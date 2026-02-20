use std::sync::Arc;

use anyhow::Result;
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};

use crate::models::transaction::Sensivity;
use crate::services::loader::LoaderTron;
use crate::services::progress::{
    save_sync_state,
    save_tx,
    save_wallet,
    save_token_transfer,
};
use crate::models::token_transfer::TokenTransferRow;

// ─── TRX sensitivity (1 TRX = 1_000_000 SUN) ────────────────────────────────

fn calc_sensivity_trx(sun: u64) -> Sensivity {
    let trx = sun as f64 / 1_000_000.0;

    if trx > 100_000.0 {
        Sensivity::Red
    } else if trx > 10_000.0 {
        Sensivity::Yellow
    } else {
        Sensivity::Green
    }
}

// ─── Tron REST API structs ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TronBlock {
    #[serde(rename = "blockID")]
    block_id: String,

    #[serde(rename = "block_header")]
    block_header: TronBlockHeader,

    #[serde(default)]
    transactions: Vec<TronTx>,
}

#[derive(Debug, Deserialize)]
struct TronBlockHeader {
    raw_data: TronBlockRaw,
}

#[derive(Debug, Deserialize)]
struct TronBlockRaw {
    number: u64,
    timestamp: u64,
}

#[derive(Debug, Deserialize)]
struct TronTx {
    txID: String,
    ret: Option<Vec<TronRet>>,
    raw_data: TronTxRaw,
}

#[derive(Debug, Deserialize)]
struct TronRet {
    contractRet: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TronTxRaw {
    contract: Vec<TronContract>,
    timestamp: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TronContract {
    #[serde(rename = "type")]
    contract_type: String,

    parameter: TronContractParameter,
}

#[derive(Debug, Deserialize)]
struct TronContractParameter {
    value: serde_json::Value,
}

// TRC20 Transfer event log
#[derive(Debug, Deserialize)]
struct TronTrc20Transfer {
    transaction_id: String,
    block_timestamp: u64,
    token_info: TronTokenInfo,
    from: String,
    to: String,
    value: String,
    token_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TronTokenInfo {
    address: String,
    name: Option<String>,
    symbol: Option<String>,
    decimals: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct TronTrc20Response {
    data: Vec<TronTrc20Transfer>,
    #[serde(default)]
    meta: Option<serde_json::Value>,
}

// ─── API Helpers ──────────────────────────────────────────────────────────────

async fn get_block(
    client: &reqwest::Client,
    base_url: &str,
    block_number: u64,
) -> Result<Option<TronBlock>> {
    let url = format!("{}/wallet/getblockbynum", base_url);

    let body = serde_json::json!({ "num": block_number });

    let resp = client.post(&url).json(&body).send().await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let block: TronBlock = resp.json().await?;
    Ok(Some(block))
}

async fn get_trc20_transfers_for_tx(
    client: &reqwest::Client,
    base_url: &str,
    tx_hash: &str,
) -> Result<Vec<TronTrc20Transfer>> {
    let url = format!(
        "{}/v1/transactions/{}/events",
        base_url, tx_hash
    );

    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        return Ok(vec![]);
    }

    let data: TronTrc20Response = resp.json().await?;
    Ok(data.data)
}

async fn get_account_info(
    client: &reqwest::Client,
    base_url: &str,
    address: &str,
) -> Result<(u64, bool)> {
    let url = format!("{}/wallet/getaccount", base_url);
    let body = serde_json::json!({ "address": address, "visible": true });

    let resp = client.post(&url).json(&body).send().await?;

    if !resp.status().is_success() {
        return Ok((0, false));
    }

    let data: serde_json::Value = resp.json().await?;

    let balance = data["balance"].as_u64().unwrap_or(0);
    // اگه code داشت یعنی smart contract هست
    let is_contract = data["contract_resource"].is_object()
        || data.get("contract").is_some();

    Ok((balance, is_contract))
}

// ─── Save Wallet ──────────────────────────────────────────────────────────────

async fn save_wallet_tron(
    client: Arc<reqwest::Client>,
    clickhouse: Arc<clickhouse::Client>,
    limiter: Arc<tokio::sync::Semaphore>,
    base_url: Arc<String>,
    address: String,
) -> Result<()> {
    if address.is_empty() || address == "T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb" {
        // آدرس zero در Tron
        return Ok(());
    }

    let (balance, is_contract) = {
        let _permit = limiter.acquire().await?;
        get_account_info(&client, &base_url, &address).await?
    };

    let wallet_type = if is_contract {
        "smart_contract".to_string()
    } else {
        "wallet".to_string()
    };

    save_wallet(
        clickhouse,
        &address,
        balance.to_string(),
        0, // Tron از nonce استفاده نمی‌کند
        wallet_type,
    )
    .await?;

    Ok(())
}

// ─── Process Transaction ──────────────────────────────────────────────────────

async fn process_tx_tron(
    client: Arc<reqwest::Client>,
    clickhouse: Arc<clickhouse::Client>,
    limiter: Arc<tokio::sync::Semaphore>,
    base_url: Arc<String>,
    tx: TronTx,
    block_number: u64,
) -> Result<Vec<String>> {
    let tx_hash = tx.txID.clone();

    // استخراج from/to/value از contract اول
    let contract = match tx.raw_data.contract.first() {
        Some(c) => c,
        None => return Ok(vec![]),
    };

    let val = &contract.parameter.value;

    let from_addr = val["owner_address"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let to_addr = val["to_address"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let amount_sun = val["amount"].as_u64().unwrap_or(0);

    save_tx(
        clickhouse.clone(),
        tx_hash.clone(),
        block_number,
        from_addr.clone(),
        to_addr.clone(),
        amount_sun.to_string(),
        calc_sensivity_trx(amount_sun) as u8,
    )
    .await?;

    // TRC20 transfers
    let trc20_transfers = {
        let _permit = limiter.acquire().await?;
        get_trc20_transfers_for_tx(&client, &base_url, &tx_hash).await?
    };

    let mut discovered_tokens: Vec<String> = vec![];

    for (log_index, transfer) in trc20_transfers.iter().enumerate() {
        let token_address = transfer.token_info.address.clone();
        discovered_tokens.push(token_address.clone());

        save_token_transfer(
            clickhouse.clone(),
            TokenTransferRow {
                tx_hash: tx_hash.clone(),
                block_number,
                log_index: log_index as u32,
                token_address,
                from_addr: transfer.from.clone(),
                to_addr: transfer.to.clone(),
                amount: transfer.value.clone(),
            },
        )
        .await?;
    }

    // ذخیره wallet ها
    if !from_addr.is_empty() {
        save_wallet_tron(
            client.clone(),
            clickhouse.clone(),
            limiter.clone(),
            base_url.clone(),
            from_addr,
        )
        .await?;
    }

    if !to_addr.is_empty() {
        save_wallet_tron(
            client.clone(),
            clickhouse.clone(),
            limiter.clone(),
            base_url.clone(),
            to_addr,
        )
        .await?;
    }

    Ok(discovered_tokens)
}

// ─── Main Fetch Function ──────────────────────────────────────────────────────
// بررسی شود این بخش . باید از یه سیستم برای دریافت client استفاده شود
pub async fn fetch_tron(
    loader: Arc<LoaderTron>,
    start_block: u64,
    total_txs: u64,
) -> Result<()> {
    let client = loader.http_client.clone();
    let clickhouse = loader.clickhouse.clone();
    let limiter = loader.rpc_limiter.clone();
    let base_url = loader.base_url.clone();

    // بلاک آخر رو از API بگیر
    let latest_block = {
        let url = format!("{}/wallet/getnowblock", base_url);
        let resp = client.post(&url).send().await?;
        let data: TronBlock = resp.json().await?;
        data.block_header.raw_data.number
    };

    println!("TRON Latest Block: {}", latest_block);

    let mut tx_count: u64 = 0;
    let mut last_synced_block: u64 = start_block;
    let mut current_block = start_block;

    while current_block <= latest_block {
        if tx_count >= total_txs {
            break;
        }

        let block_opt = {
            let _permit = limiter.acquire().await?;
            get_block(&client, &base_url, current_block).await?
        };

        let Some(block) = block_opt else {
            current_block += 1;
            continue;
        };

        let txs = block.transactions;

        if txs.is_empty() {
            println!(
                "[TRON] Block {} has 0 txs (valid empty block)",
                current_block
            );

            last_synced_block = current_block;
            save_sync_state(clickhouse.clone(), "tron", last_synced_block).await?;

            current_block += 1;
            continue;
        }

        let mut tasks = FuturesUnordered::new();
        let mut discovered_tokens_all: Vec<String> = vec![];
        let mut fully_processed_block = true;

        for tx in txs {
            if tx_count >= total_txs {
                fully_processed_block = false;
                break;
            }

            let client = client.clone();
            let clickhouse = clickhouse.clone();
            let limiter = limiter.clone();
            let base_url = Arc::new(base_url.clone());
            let block_number = current_block;

            tasks.push(tokio::spawn(async move {
                process_tx_tron(client, clickhouse, limiter, base_url, tx, block_number).await
            }));

            tx_count += 1;
            println!("[TRON] --> Queued tx #{}", tx_count);
        }

        while let Some(res) = tasks.next().await {
            let tokens = res??;
            discovered_tokens_all.extend(tokens);
        }

        // Token metadata (اگه داری token_metadata_worker برای Tron هم بسازی)
        // tron_token_metadata_worker::process_new_tokens(...).await?;

        if fully_processed_block {
            last_synced_block = current_block;

            save_sync_state(clickhouse.clone(), "tron", last_synced_block).await?;

            println!(
                "TRON synced block {} | total tx processed {}",
                last_synced_block, tx_count
            );
        } else {
            println!(
                "TRON stopped mid-block {} (tx limit reached) | total tx processed {}",
                current_block, tx_count
            );
            break;
        }

        current_block += 1;
    }

    save_sync_state(clickhouse.clone(), "tron", last_synced_block).await?;

    Ok(())
}
