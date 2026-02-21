use clickhouse::Row;
use serde::Serialize;

#[derive(Debug, Row, Serialize)]
pub struct ContractCallRow {
    pub tx_hash: String,
    pub contract_address: String,
    pub method: String,
    pub data: String,
}