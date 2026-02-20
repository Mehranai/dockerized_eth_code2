use anyhow::{Result, anyhow};
use serde_json::Value;
use reqwest::Client;

pub struct TronClient {
    base_url: String,
    http: Client,
}

impl TronClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            http: Client::new(),
        }
    }

    pub async fn get_block_number(&self) -> Result<u64> {
        let url = format!("{}/wallet/getnowblock", self.base_url);

        let resp = self.http
            .post(&url)
            .send()
            .await?;

        let json: Value = resp.json().await?;

        let number = json["block_header"]["raw_data"]["number"]
            .as_u64()
            .ok_or_else(|| anyhow!("Invalid TRON block response"))?;

        Ok(number)
    }
}