use crate::models::TokenTransferRow;
use crate::models::wallet::WalletRow;
use crate::models::owner::OwnerRow;
use crate::models::transaction::TransactionRow;
use crate::models::token_metadata::TokenMetadataRow;
use crate::models::contract_call::ContractCallRow;
use crate::models::money_flow::MoneyFlowRow;

use clickhouse::Client;
use std::sync::Arc;
use anyhow::Result;
use nanoid::nanoid;

// TRANSACTION

pub async fn save_tx(
    clickhouse: Arc<Client>,
    hash: String,
    block_number: u64,
    from: String,
    to: String,
    value: String,
    sensivity: u8,
) -> Result<()> {

    let tx_row = TransactionRow {
        hash,
        block_number,
        from_addr: from,
        to_addr: to,
        value,
        sensivity,
    };

    let mut insert = clickhouse.insert::<TransactionRow>("transactions").await?;
    insert.write(&tx_row).await?;
    insert.end().await?;

    Ok(())
}

// WALLET + OWNER (AUTO TAGGING)

pub async fn save_wallet(
    clickhouse: Arc<Client>,
    addr: &str,
    balance: String,
    nonce: u64,
    mwallet_type: String,
) -> Result<()> {

    if addr.is_empty() {
        return Ok(());
    }

    // تشخیص نوع آدرس (wallet / exchange / contract)
    let wallet_type = detect_wallet_type(&clickhouse, addr, nonce, mwallet_type).await?;

    // تعیین person_id
    let person_id = match wallet_type.as_str() {
        "exchange" => format!("EXCHANGE_{}", addr), // entity مشترک
        _ => get_or_create_person_id(&clickhouse, addr).await?,
    };

    let wallet = WalletRow {
        address: addr.to_string(),
        balance,
        nonce,
        wallet_type: wallet_type.clone(),
        person_id: person_id.clone(),
    };

    let owner = OwnerRow {
        address: addr.to_string(),
        person_name: "".into(),
        person_id,
        personal_id: 0,
    };

    // wallet_info
    let mut insert_wallet = clickhouse.insert::<WalletRow>("wallet_info").await?;
    insert_wallet.write(&wallet).await?;
    insert_wallet.end().await?;

    // owner_info
    let mut insert_owner = clickhouse.insert::<OwnerRow>("owner_info").await?;
    insert_owner.write(&owner).await?;
    insert_owner.end().await?;

    Ok(())
}


// AUTO TAGGING LOGIC

async fn detect_wallet_type(
    clickhouse: &Client,
    address: &str,
    nonce: u64,
    detect_wallet_type: String,
) -> Result<String> {

    // Known exchange DB
    let known_exchange: u64 = clickhouse
        .query(
            "SELECT count()
             FROM address_tags
             WHERE address = ? AND tag = 'EXCHANGE'"
        )
        .bind(address)
        .fetch_one::<u64>()
        .await?;

    if known_exchange > 0 {
        return Ok("exchange".into());
    }

    // nonce heuristic
    if nonce > 10_000 {
        return Ok("exchange".into());
    }

    // fan-in heuristic
    let fan_in: u64 = clickhouse
        .query(
            "SELECT countDistinct(from_addr)
             FROM transactions
             WHERE to_addr = ?"
        )
        .bind(address)
        .fetch_one::<u64>()
        .await?;

    if fan_in > 500 {
        return Ok("exchange".into());
    }

    Ok(detect_wallet_type)
}


// PERSON ID MANAGEMENT

async fn get_or_create_person_id(
    clickhouse: &Client,
    address: &str,
) -> Result<String> {

    let existing = clickhouse
        .query(
            "SELECT person_id
             FROM wallet_info
             WHERE address = ?
             LIMIT 1"
        )
        .bind(address)
        .fetch_optional::<String>()
        .await?;

    if let Some(row) = existing {
        return Ok(row);
    }

    Ok(generate_person_id())
}


pub fn generate_person_id() -> String {
    nanoid!(10)
}

// Balance of tokens section

// TOKEN TRANSFERS (ONLY CANONICAL INSERT)
// Start of Ethereum section

pub async fn save_token_transfer(
    clickhouse: Arc<Client>,
    row: TokenTransferRow,
) -> Result<()> {

    let mut insert =
        clickhouse.insert::<TokenTransferRow>("token_transfers").await?;

    insert.write(&row).await?;
    insert.end().await?;

    Ok(())
}

pub async fn save_token_metadata(
    clickhouse: Arc<Client>,
    row: TokenMetadataRow,
) -> Result<()> {

    let mut insert = clickhouse
        .insert::<TokenMetadataRow>("token_metadata")
        .await?;

    insert.write(&row).await?;
    insert.end().await?;

    Ok(())
}
// End of Ethereum section

// SYNC STATE

#[derive(Debug, clickhouse::Row, serde::Serialize)]
pub struct SyncStateRow {
    pub chain: String,
    pub last_synced_block: u64,
}

pub async fn save_sync_state(
    clickhouse: Arc<Client>,
    chain: &str,
    last_synced_block: u64,
) -> Result<()> {

    let row = SyncStateRow {
        chain: chain.to_string(),
        last_synced_block,
    };

    let mut insert = clickhouse.insert::<SyncStateRow>("sync_state").await?;
    insert.write(&row).await?;
    insert.end().await?;

    Ok(())
}

// Tron Section
pub async fn save_contract_call(
    clickhouse: Arc<Client>,
    row: ContractCallRow,
) -> Result<()> {
    let mut insert = clickhouse
        .insert::<ContractCallRow>("contract_calls")
        .await?;

    insert.write(&row).await?;
    insert.end().await?;

    Ok(())
}

pub async fn save_money_flow(
    clickhouse: Arc<Client>,
    row: MoneyFlowRow,
) -> Result<()> {
    let mut insert = clickhouse
        .insert::<MoneyFlowRow>("money_flows")
        .await?;

    insert.write(&row).await?;
    insert.end().await?;

    Ok(())
}