use std::{time::{Duration, Instant}, str::FromStr, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use reqwest::Client;
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use tokio::sync::Mutex;

use super::datasource::{TickerData, TickerDataSource};


pub struct CoinbaseTickerDataSource {
    client: Arc<Client>,
    ticker: String,
    last_check_res: Mutex<Option<(Instant, TickerData)>>,
}

impl CoinbaseTickerDataSource {
    pub fn new(client: Arc<Client>, ticker: String) -> CoinbaseTickerDataSource {
        CoinbaseTickerDataSource {
            client,
            ticker,
            last_check_res: Mutex::new(None),
        }
    }

    async fn run_query(&self) -> Result<(Decimal, Decimal)> {
        let resp_payload = self.client
            .get(&format!("https://api.exchange.coinbase.com/products/{}/stats", &self.ticker))
            .send()
            .await?;
        let response: JsonValue = resp_payload.json().await?;
        info!("Coinbase: {} {}", &self.ticker, response);
        if response["message"] != JsonValue::Null {
            return Err(anyhow!("Coinbase: {}", response["message"]));
        }
        let last = Decimal::from_str(
            response["last"]
                .as_str()
                .ok_or(anyhow!("Failed to parse Coinbase response"))?,
        )?;
        let open = Decimal::from_str(
            response["open"]
                .as_str()
                .ok_or(anyhow!("Failed to parse Coinbase response"))?,
        )?;
        Ok((last, open))
    }

    // async fn query_price(&self) -> Result<Decimal> {
    //     let response: JsonValue = self.client
    //         .get(&format!("https://api.exchange.coinbase.com/products/{}/ticker", &self.ticker))
    //         .send()
    //         .await?
    //         .json()
    //         .await?;
    //     if response["message"] != JsonValue::Null {
    //         return Err(anyhow!("Coinbase: {}", response["message"]));
    //     }
    //     let price = Decimal::from_str(
    //         response["price"]
    //             .as_str()
    //             .ok_or(anyhow!("Failed to parse Coinbase ticker response"))?,
    //     )?;
    //     Ok(price)
    // }

    // async fn query_6hr(&self) -> Result<Decimal> {
    //     let now_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    //     let prev_ts = now_ts - 6 * 3600;
    //     let response: JsonValue = self.client
    //         .get(&format!("https://api.exchange.coinbase.com/products/{}/candles?granularity=60&start={}&end={}", &self.ticker, prev_ts - 60, prev_ts))
    //         .send()
    //         .await?
    //         .json()
    //         .await?;
    //     if response["message"] != JsonValue::Null {
    //         return Err(anyhow!("Coinbase: {}", response["errors"][0]["message"]));
    //     }
    //     let prev = Decimal::from_str(
    //         response[0][4]  // [timestamp, price_low, price_high, price_open, price_close]
    //             .as_str()
    //             .ok_or(anyhow!("Failed to parse Coinbase candle response"))?,
    //     )?;
    //     Ok(prev)
    // }
}

#[async_trait]
impl TickerDataSource for CoinbaseTickerDataSource {
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