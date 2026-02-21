use anyhow::{Result, anyhow, Context};
use serde_json::Value;
use reqwest::Client;
// use std::time::Duration; // benazar Tokio behtare
use tokio::time::{sleep, Duration};

use reqwest::header::{HeaderMap, HeaderValue};

pub struct TronClient {
    base_url: String,
    http: Client,
}

impl TronClient {
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let mut headers = HeaderMap::new();

        if let Some(key) = api_key {
            headers.insert(
                "TRON-PRO-API-KEY",
                HeaderValue::from_str(&key).unwrap(),
            );
        }

        let http = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .unwrap();

        Self { http, base_url }
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

        let mut last_err = None;

        for attempt in 1..=3 {
            let resp = self.http
                .post(&url)
                .json(&serde_json::json!({ "num": block }))
                .send()
                .await;

            match resp {
                Ok(r) => {
                    let json: Value = r.json().await?;
                    return Ok(json);
                }
                Err(e) => {
                    last_err = Some(e);
                    let delay = attempt * 2;
                    eprintln!(
                        "[TRON] retry getblock {} (attempt {})",
                        block, attempt
                    );
                    sleep(Duration::from_secs(delay)).await;
                }
            }
        }

        Err(anyhow!(
            "getblockbynum failed after retries for {}: {:?}",
            block,
            last_err
        ))
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
