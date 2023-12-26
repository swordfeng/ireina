use std::{time::{Duration, Instant}, str::FromStr, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

use super::datasource::{TickerData, TickerDataSource};


pub struct BinanceTickerDataSource {
    client: Arc<Client>,
    ticker: String,
    last_check_res: Mutex<Option<(Instant, TickerData)>>,
}

impl BinanceTickerDataSource {
    pub fn new(client: Arc<Client>, ticker: String) -> BinanceTickerDataSource {
        BinanceTickerDataSource {
            client,
            ticker,
            last_check_res: Mutex::new(None),
        }
    }

    async fn run_query(&self) -> Result<(Decimal, Decimal)> {
        let resp_payload = self.client
            .get(&format!("https://api-gcp.binance.com/api/v3/ticker/24hr?symbol={}&type=MINI", &self.ticker))
            .send()
            .await?;
        info!("Binance response code: {}", resp_payload.status());
        let response: JsonValue = resp_payload.json().await?;
        info!("Binance: {} {}", &self.ticker, response);
        if response["msg"] != JsonValue::Null {
            return Err(anyhow!("Binance: {}", response["msg"]));
        }
        let last = Decimal::from_str(
            response["lastPrice"]
                .as_str()
                .ok_or(anyhow!("Failed to parse Binance response"))?,
        )?;
        let open = Decimal::from_str(
            response["openPrice"]
                .as_str()
                .ok_or(anyhow!("Failed to parse Binance response"))?,
        )?;
        Ok((last, open))
    }
}

#[async_trait]
impl TickerDataSource for BinanceTickerDataSource {
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