use std::{
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

use super::datasource::{TickerData, TickerDataSource};

pub struct KrakenTickerDataSource {
    client: Arc<Client>,
    ticker: String,
    last_check_res: Mutex<Option<(Instant, TickerData)>>,
}

impl KrakenTickerDataSource {
    pub fn new(client: Arc<Client>, ticker: String) -> KrakenTickerDataSource {
        KrakenTickerDataSource {
            client,
            ticker,
            last_check_res: Mutex::new(None),
        }
    }

    async fn run_query(&self) -> Result<Decimal> {
        let response: JsonValue = self
            .client
            .get(format!(
                "https://api.kraken.com/0/public/Ticker?pair={}",
                &self.ticker
            ))
            .send()
            .await?
            .json()
            .await?;
        info!("Kraken: {} {}", &self.ticker, response);
        if response["error"][0] != JsonValue::Null {
            return Err(anyhow!("Kraken: {}", response["error"][0]));
        }
        let last = Decimal::from_str(
            response["result"][&self.ticker]["c"][0]
                .as_str()
                .ok_or(anyhow!("Failed to parse Kraken response"))?,
        )?;
        Ok(last)
    }
}

#[async_trait]
impl TickerDataSource for KrakenTickerDataSource {
    async fn get_ticker_data(&self) -> TickerData {
        let mut last_check_res = self.last_check_res.lock().await;
        if let Some((ref time, ref ticker_data)) = *last_check_res {
            if time.elapsed() < Duration::from_secs(5) {
                return ticker_data.clone();
            }
        }
        match self.run_query().await {
            Ok(last_price) => {
                let ticker_data = TickerData {
                    last_price: Some(last_price),
                    prev_price: None,
                    insufficient_data: false,
                    errors: vec![],
                };
                *last_check_res = Some((Instant::now(), ticker_data.clone()));
                ticker_data
            }
            Err(e) => TickerData {
                last_price: None,
                prev_price: None,
                insufficient_data: true,
                errors: vec![e.to_string()],
            },
        }
    }
}
