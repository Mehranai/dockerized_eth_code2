use std::sync::Arc;

use anyhow::{Result, Context};
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;

use crate::services::loader::LoaderTron;
use crate::services::progress::{
    save_tx,
    save_wallet,
    save_sync_state,
    save_token_transfer,
};
use crate::models::transaction::Sensivity;
use crate::models::token_transfer::TokenTransferRow;

const TRC20_TRANSFER_TOPIC: &str =
    "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

/// ------------------------------------------------
/// Utils
/// ------------------------------------------------

fn calc_sensivity_trx(amount_sun: i64) -> Sensivity {
    let trx = amount_sun as f64 / 1_000_000.0;

    if trx >= 1_000_000.0 {
        Sensivity::Red
    } else if trx >= 100_000.0 {
        Sensivity::Yellow
    } else {
        Sensivity::Green
    }
}

fn parse_trx_transfer(tx: &Value) -> Option<(String, String, i64)> {
    let contract = tx["raw_data"]["contract"].get(0)?;
    let value = &contract["parameter"]["value"];

    Some((
        value["owner_address"].as_str()?.to_string(),
        value["to_address"].as_str()?.to_string(),
        value["amount"].as_i64()?,
    ))
}

fn parse_trc20_transfers(
    tx_hash: &str,
    tx_info: &Value,
    block_number: u64,
) -> Vec<TokenTransferRow> {

    let mut rows = Vec::new();

    let logs = match tx_info["log"].as_array() {
        Some(l) => l,
        None => return rows,
    };

    for (idx, log) in logs.iter().enumerate() {
        let topics = match log["topics"].as_array() {
            Some(t) if t.len() == 3 => t,
            _ => continue,
        };

        if topics[0].as_str() != Some(TRC20_TRANSFER_TOPIC) {
            continue;
        }

        let amount = i64::from_str_radix(
            log["data"].as_str().unwrap_or("0"),
            16,
        )
        .unwrap_or(0);

        rows.push(TokenTransferRow {
            tx_hash: tx_hash.to_string(),
            block_number,
            log_index: idx as u32,
            token_address: log["address"].as_str().unwrap_or_default().to_string(),
            from_addr: topics[1].as_str().unwrap_or_default().to_string(),
            to_addr: topics[2].as_str().unwrap_or_default().to_string(),
            amount: amount.to_string(),
        });
    }

    rows
}

/// ------------------------------------------------
/// TX processing
/// ------------------------------------------------

async fn process_tron_tx(
    loader: Arc<LoaderTron>,
    tx: Value,
    block_number: u64,
) -> Result<()> {

    let tx_hash = tx["txID"]
        .as_str()
        .context("missing txID")?
        .to_string();

    // ---------- TRX ----------
    if let Some((from, to, amount)) = parse_trx_transfer(&tx) {
        save_tx(
            loader.clickhouse.clone(),
            tx_hash.clone(),
            block_number,
            from.clone(),
            to.clone(),
            amount.to_string(),
            calc_sensivity_trx(amount) as u8,
        ).await?;

        save_wallet(loader.clickhouse.clone(), &from, "0".into(), 0, "wallet".into()).await?;
        save_wallet(loader.clickhouse.clone(), &to, "0".into(), 0, "wallet".into()).await?;
    }

    // ---------- TRC20 ----------
    let tx_info = loader
        .tron_client
        .get_transaction_info(&tx_hash)
        .await
        .with_context(|| format!("failed tx info {}", tx_hash))?;

    for row in parse_trc20_transfers(&tx_hash, &tx_info, block_number) {
        save_token_transfer(loader.clickhouse.clone(), row).await?;
    }

    Ok(())
}

/// ------------------------------------------------
/// Main loop (ETH‑style)
/// ------------------------------------------------

pub async fn fetch_tron(
    loader: Arc<LoaderTron>,
    start_block: u64,
    max_txs: u64,
) -> Result<()> {

    let latest = loader.tron_client.get_block_number().await?;
    let mut current = start_block;
    let mut processed = 0_u64;

    println!("[TRON] start {} → {}", start_block, latest);

    while current <= latest && processed < max_txs {
        let block = loader
            .tron_client
            .get_block_by_number(current)
            .await
            .with_context(|| format!("block {}", current))?;

        let txs = block["transactions"].as_array();

        if txs.is_none() || txs.unwrap().is_empty() {
            save_sync_state(loader.clickhouse.clone(), "tron", current).await?;
            current += 1;
            continue;
        }

        let mut tasks = FuturesUnordered::new();

        for tx in txs.unwrap() {
            if processed >= max_txs {
                break;
            }

            let loader = loader.clone();
            let tx = tx.clone();

            tasks.push(tokio::spawn(async move {
                process_tron_tx(loader, tx, current).await
            }));

            processed += 1;
        }

        while let Some(res) = tasks.next().await {
            res??;
        }

        save_sync_state(loader.clickhouse.clone(), "tron", current).await?;
        println!("[TRON] block {} synced | txs {}", current, processed);

        current += 1;
    }

    Ok(())
}
