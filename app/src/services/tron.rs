use std::sync::Arc;

use anyhow::{Context, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use serde_json::Value;

use crate::models::contract_call::ContractCallRow;
use crate::models::money_flow::MoneyFlowRow;
use crate::models::token_transfer::TokenTransferRow;
use crate::services::loader::LoaderTron;
use crate::services::progress::{
    save_contract_call,
    save_money_flow,
    save_sync_state,
    save_token_transfer,
    save_tx,
    save_wallet,
};

const TRC20_TRANSFER_TOPIC: &str =
    "ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

/// -----------------------------------------------------
/// TX PROCESSOR
/// -----------------------------------------------------
async fn process_tx(
    loader: Arc<LoaderTron>,
    tx: Value,
    block_number: u64,
) -> Result<()> {
    let tx_id = tx["txID"]
        .as_str()
        .context("txID missing")?
        .to_string();

    let raw = &tx["raw_data"];
    let contract = &raw["contract"][0];
    let contract_type = contract["type"].as_str().unwrap_or("");
    let value = &contract["parameter"]["value"];

    let owner = value["owner_address"].as_str().unwrap_or("").to_string();
    let to = value["to_address"].as_str().unwrap_or("").to_string();
    let call_value = value["call_value"].as_i64().unwrap_or(0);

    // ---------- transaction ----------
    save_tx(
        loader.clickhouse.clone(),
        tx_id.clone(),
        block_number,
        owner.clone(),
        to.clone(),
        call_value.to_string(),
        0,
    )
    .await?;

    // ---------- native TRX flow ----------
    if call_value > 0 && !to.is_empty() {
        save_money_flow(
            loader.clickhouse.clone(),
            MoneyFlowRow {
                tx_hash: tx_id.clone(),
                from_addr: owner.clone(),
                to_addr: to.clone(),
                amount: call_value.to_string(),
                asset: "TRX".to_string(),
            },
        )
        .await?;
    }

    // ---------- receipt ----------
    let receipt = loader
        .tron_client
        .get_transaction_info(&tx_id)
        .await?;

    // ---------- TRC20 logs ----------
    if let Some(logs) = receipt["log"].as_array() {
        let empty_topics: Vec<Value> = Vec::new();

        for (idx, log) in logs.iter().enumerate() {
            let topics = log["topics"].as_array().unwrap_or(&empty_topics);

            if topics.len() == 3
                && topics[0].as_str().unwrap_or("") == TRC20_TRANSFER_TOPIC
            {
                let token_address = log["address"].as_str().unwrap_or("").to_string();
                let from_addr = topics[1].as_str().unwrap_or("").to_string();
                let to_addr = topics[2].as_str().unwrap_or("").to_string();
                let amount = log["data"].as_str().unwrap_or("0").to_string();

                save_token_transfer(
                    loader.clickhouse.clone(),
                    TokenTransferRow {
                        tx_hash: tx_id.clone(),
                        block_number,
                        log_index: idx as u32,
                        token_address: token_address.clone(),
                        from_addr: from_addr.clone(),
                        to_addr: to_addr.clone(),
                        amount: amount.clone(),
                    },
                )
                .await?;

                save_money_flow(
                    loader.clickhouse.clone(),
                    MoneyFlowRow {
                        tx_hash: tx_id.clone(),
                        from_addr,
                        to_addr,
                        amount,
                        asset: token_address,
                    },
                )
                .await?;
            }
        }
    }

    // ---------- contract calls ----------
    if contract_type == "TriggerSmartContract" {
        let contract_address =
            value["contract_address"].as_str().unwrap_or("").to_string();
        let data = value["data"].as_str().unwrap_or("").to_string();
        let method = data.get(0..8).unwrap_or("unknown").to_string();

        save_contract_call(
            loader.clickhouse.clone(),
            ContractCallRow {
                tx_hash: tx_id.clone(),
                contract_address,
                method,
                data,
            },
        )
        .await?;
    }

    // ---------- wallets ----------
    save_wallet(
        loader.clickhouse.clone(),
        &owner,
        "0".to_string(),
        0,
        "wallet".to_string(),
    )
    .await?;

    if !to.is_empty() {
        save_wallet(
            loader.clickhouse.clone(),
            &to,
            "0".to_string(),
            0,
            "wallet".to_string(),
        )
        .await?;
    }

    Ok(())
}

/// -----------------------------------------------------
/// BLOCK LOOP
/// -----------------------------------------------------
pub async fn fetch_tron(
    loader: Arc<LoaderTron>,
    start_block: u64,
    total_tron_txs: Option<u64>,
) -> Result<()> {
    let latest_block = loader.tron_client.get_block_number().await?;
    let mut current_block = start_block;
    let mut processed: u64 = 0;

    while current_block <= latest_block {
        let block = loader
            .tron_client
            .get_block_by_number(current_block)
            .await?;

        let empty: Vec<Value> = Vec::new();
        let txs = block["transactions"].as_array().unwrap_or(&empty);

        let mut tasks = FuturesUnordered::new();

        for tx in txs {
            if let Some(limit) = total_tron_txs {
                if processed >= limit {
                    break;
                }
            }
            processed += 1;

            let tx_owned = tx.clone();
            let loader_cloned = loader.clone();
            let block_num = current_block;

            tasks.push(tokio::spawn(async move {
                let _permit =
                    loader_cloned.rpc_limiter.acquire().await.unwrap();
                process_tx(loader_cloned.clone(), tx_owned, block_num).await
            }));
        
            println!("[Tron] --> Queued tx #{}", processed);
        }

        while let Some(res) = tasks.next().await {
            res??;
        }

        save_sync_state(
            loader.clickhouse.clone(),
            "tron",
            current_block,
        )
        .await?;

        current_block += 1;
    }

    Ok(())
}
