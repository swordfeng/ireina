use std::{time::{Duration, Instant}, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use reqwest::Client;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

use super::datasource::{TickerData, TickerDataSource};


pub struct GoldpriceTickerDataSource {
    client: Arc<Client>,
    metal: String,
    currency: String,
    last_check_res: Mutex<Option<(Instant, TickerData)>>,
}

impl GoldpriceTickerDataSource {
    pub fn new(client: Arc<Client>, metal: String, currency: String) -> GoldpriceTickerDataSource {
        GoldpriceTickerDataSource {
            client,
            metal,
            currency,
            last_check_res: Mutex::new(None),
        }
    }

    async fn run_query(&self) -> Result<(Decimal, Decimal)> {
        let response: JsonValue = self.client
            .get(&format!("https://data-asg.goldprice.org/dbXRates/{}", &self.currency))
            .send()
            .await?
            .json()
            .await?;
        info!("Goldprice: {}", response);
        let last = response["items"][0][self.metal.to_ascii_lowercase() + "Price"]
            .as_f64()
            .and_then(Decimal::from_f64)
            .ok_or(anyhow!("Failed to parse Goldprice response"))?;
        let prev = response["items"][0][self.metal.to_ascii_lowercase() + "Close"]
            .as_f64()
            .and_then(Decimal::from_f64)
            .ok_or(anyhow!("Failed to parse Goldprice response"))?;
        Ok((last, prev))
    }
}

#[async_trait]
impl TickerDataSource for GoldpriceTickerDataSource {
    async fn get_ticker_data(&self) -> TickerData {
        let mut last_check_res = self.last_check_res.lock().await;
        if let Some((ref time, ref ticker_data)) = *last_check_res {
            if time.elapsed() < Duration::from_secs(5) {
                return ticker_data.clone()
            }
        }
        match self.run_query().await {
            Ok((last_price, prev_price)) => {
                let ticker_data = TickerData {
                    last_price: Some(last_price),
                    prev_price: Some(prev_price),
                    insufficient_data: false,
                    errors: vec![],
                };
                *last_check_res = Some((Instant::now(), ticker_data.clone()));
                ticker_data
            },
            Err(e) => TickerData {
                last_price: None,
                prev_price: None,
                insufficient_data: true,
                errors: vec![e.to_string()]
            }
        }
    }
}