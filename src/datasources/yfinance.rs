use std::{time::{Duration, Instant}, sync::Arc};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::info;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use tokio::sync::Mutex;

use super::datasource::{TickerData, TickerDataSource};


pub struct YahooFinanceTickerDataSource {
    connector: Arc<yahoo_finance_api::YahooConnector>,
    ticker: String,
    last_check_res: Mutex<Option<(Instant, TickerData)>>,
}

impl YahooFinanceTickerDataSource {
    pub fn new(connector: Arc<yahoo_finance_api::YahooConnector>, ticker: String) -> YahooFinanceTickerDataSource {
        YahooFinanceTickerDataSource {
            connector,
            ticker,
            last_check_res: Mutex::new(None),
        }
    }

    async fn run_query(&self) -> Result<(Option<Decimal>, Option<Decimal>)> {
        let quotes = self.connector.get_quote_range(&self.ticker, "1d", "5d")
            .await?
            .quotes()?
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        info!("Yahoo: {} {:?}", &self.ticker, &quotes);
        let prev_price = if quotes.len() >= 2 {
            quotes[1].adjclose
        } else {
            0.
        };
        if quotes.len() == 0 {
            Ok((None, None))
        } else if prev_price == 0. {
            let last = Decimal::from_f64(quotes[0].close).ok_or(anyhow!("Failed to parse yfi price into decimal"))?;
            Ok((Some(last), None))
        } else {
            let last = Decimal::from_f64(quotes[0].close).ok_or(anyhow!("Failed to parse yfi price into decimal"))?;
            let prev = Decimal::from_f64(prev_price).ok_or(anyhow!("Failed to parse yfi price into decimal"))?;
            Ok((Some(last), Some(prev)))
        }
    }
}

#[async_trait]
impl TickerDataSource for YahooFinanceTickerDataSource {
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
                    last_price,
                    prev_price,
                    insufficient_data: last_price.is_none(),
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