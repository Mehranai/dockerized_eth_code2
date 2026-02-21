use anyhow::{Result, anyhow, Context};
use serde_json::Value;
use reqwest::Client;
use std::time::Duration;

pub struct TronClient {
    base_url: String,
    http: Client,
}

impl TronClient {
    pub fn new(base_url: &str) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build reqwest client");

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        }
    }

    /// Latest block number
    pub async fn get_block_number(&self) -> Result<u64> {
        let url = format!("{}/wallet/getnowblock", self.base_url);

        let resp = self.http
            .post(&url)
            .send()
            .await
            .context("getnowblock request failed")?;

        let json: Value = resp.json().await?;

        json["block_header"]["raw_data"]["number"]
            .as_u64()
            .ok_or_else(|| anyhow!("Invalid getnowblock response"))
    }

    /// Full block by number
    pub async fn get_block_by_number(&self, block: u64) -> Result<Value> {
        let url = format!("{}/wallet/getblockbynum", self.base_url);

        let resp = self.http
            .post(&url)
            .json(&serde_json::json!({ "num": block }))
            .send()
            .await
            .with_context(|| format!("getblockbynum failed for {}", block))?;

        let json: Value = resp.json().await?;
        Ok(json)
    }

    /// Transaction receipt (logs, status, etc.)
    pub async fn get_transaction_info(&self, tx_id: &str) -> Result<Value> {
        let url = format!("{}/wallet/gettransactioninfobyid", self.base_url);

        let resp = self.http
            .post(&url)
            .json(&serde_json::json!({ "value": tx_id }))
            .send()
            .await
            .with_context(|| format!("gettransactioninfobyid failed: {}", tx_id))?;

        let json: Value = resp.json().await?;
        Ok(json)
    }
}
