use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use log::{info, error};
use reqwest::Client;
use serde_json::{Value as JsonValue, json};
use tokio::sync::Mutex;

pub struct CoinbaseMonitor {
    client: Arc<Client>,
    data: Mutex<JsonValue>,
}

impl CoinbaseMonitor {
    pub fn new(client: Arc<Client>) -> CoinbaseMonitor {
        CoinbaseMonitor {
            client,
            data: Mutex::new(json!([])),
        }
    }

    pub async fn monitor(&self) -> () {
       loop {
            match self.query_products().await {
                Ok(products) => {
                    *(self.data.lock().await) = products;
                },
                Err(err) => error!("{}", err)
            }
            tokio::time::sleep(Duration::from_secs(300)).await;
        }
    }

    pub async fn query(&self, ticker: &str) -> Option<JsonValue> {
        let data = self.data.lock().await;
        for product in data.as_array().expect("data must be json array") {
            if product["base_currency"] == ticker && product["quote_currency"] == "USD" {
                return Some(product.to_owned())
            }
        }
        None
    }

    async fn query_products(&self) -> Result<JsonValue> {
        info!("Querying Coinbase products");
        let resp_payload = self.client
            .get(&format!("https://api.exchange.coinbase.com/products"))
            .send()
            .await?;
        let response: JsonValue = resp_payload.json().await?;
        // info!("Coinbase products: {}", &response);
        if response["message"] != JsonValue::Null {
            return Err(anyhow!("Coinbase monitor: {}", response["message"]));
        }
        if !response.is_array() {
            return Err(anyhow!("Coinbase monitor: result is not array"));
        }
        Ok(response)
    }
}