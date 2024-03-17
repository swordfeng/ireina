use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{anyhow, Result};
use json_structural_diff::JsonDiff;
use log::{error, info};
use pretty_duration::pretty_duration;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

pub struct CoinbaseMonitor {
    client: Arc<Client>,
    data: Mutex<Vec<(SystemTime, BTreeMap<String, Product>)>>,
}

impl CoinbaseMonitor {
    pub fn new(client: Arc<Client>) -> CoinbaseMonitor {
        CoinbaseMonitor {
            client,
            data: Mutex::new(vec![]),
        }
    }

    pub async fn monitor(&self) -> () {
        loop {
            let now = SystemTime::now();
            match self.query_products().await {
                Ok(products) => {
                    let products = products
                        .as_array()
                        .unwrap()
                        .into_iter()
                        .cloned()
                        .filter_map(|p| serde_json::from_value(p).ok())
                        .map(|p: Product| (p.id.clone(), p))
                        .collect::<BTreeMap<_, _>>();
                    let mut data = self.data.lock().await;
                    let mut updated = data
                        .iter()
                        .filter(|(time, _)| &(now - Duration::from_secs(3600 * 24)) <= time)
                        .cloned()
                        .collect::<Vec<_>>();
                    updated.push((now, products));
                    *data = updated;
                }
                Err(err) => error!("{}", err),
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }

    pub async fn query(&self, ticker: &str) -> Option<Product> {
        let data = self.data.lock().await;
        data.last()
            .and_then(|(_, products)| products.get(&format!("{}-USD", ticker)).cloned())
    }

    pub async fn query_cmp(&self) -> Option<String> {
        let data = self.data.lock().await;
        if let (Some((stime, sproducts)), Some((etime, eproducts))) = (data.first(), data.last()) {
            JsonDiff::diff_string(
                &serde_json::to_value(sproducts).unwrap(),
                &serde_json::to_value(eproducts).unwrap(),
                false,
            )
            .map(|res| {
                let now = SystemTime::now();
                let update_duration = now
                    .duration_since(*etime)
                    .map(|d| pretty_duration(&d, None))
                    .unwrap_or("[error]".to_owned());
                let compare_duration = now
                    .duration_since(*stime)
                    .map(|d| pretty_duration(&d, None))
                    .unwrap_or("[error]".to_owned());
                format!(
                    "{}\nUpdated: {} ago\nComparing to: {} ago",
                    res, update_duration, compare_duration
                )
            })
        } else {
            None
        }
    }

    async fn query_products(&self) -> Result<JsonValue> {
        info!("Querying Coinbase products");
        let resp_payload = self
            .client
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    id: String,
    base_currency: String,
    quote_currency: String,
    display_name: String,
    #[serde(flatten)]
    other: BTreeMap<String, JsonValue>,
}
