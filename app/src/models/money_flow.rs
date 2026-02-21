use clickhouse::Row;
use serde::Serialize;

#[derive(Debug, Row, Serialize)]
pub struct MoneyFlowRow {
    pub tx_hash: String,
    pub from_addr: String,
    pub to_addr: String,
    pub amount: String,
    pub asset: String,
}