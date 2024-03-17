use async_trait::async_trait;
use futures::future::join_all;
use rust_decimal::{prelude::FromPrimitive, Decimal};

use super::datasource::{TickerData, TickerDataSource};

pub struct Aggregator {
    sources: Vec<Box<dyn TickerDataSource + Sync>>,
}

impl Aggregator {
    pub fn new(sources: Vec<Box<dyn TickerDataSource + Sync>>) -> Aggregator {
        Aggregator { sources }
    }
}

#[async_trait]
impl TickerDataSource for Aggregator {
    async fn get_ticker_data(&self) -> TickerData {
        let prices = join_all(self.sources.iter().map(|s| s.get_ticker_data())).await;
        let last_price_vec: Vec<_> = prices.iter().flat_map(|t| t.last_price).collect();
        let prev_price_vec: Vec<_> = prices.iter().flat_map(|t| t.prev_price).collect();
        TickerData {
            last_price: median(last_price_vec.iter().cloned()),
            prev_price: median(prev_price_vec.iter().cloned()),
            insufficient_data: self.sources.len() == 0
                || (last_price_vec.len() < self.sources.len() && last_price_vec.len() < 3),
            errors: prices
                .iter()
                .flat_map(|t| t.errors.iter().cloned())
                .collect(),
        }
    }
}

fn median(data: impl Iterator<Item = Decimal>) -> Option<Decimal> {
    let mut data: Vec<_> = data.collect();
    data.sort();
    let size = data.len();
    if size == 0 {
        return None;
    }
    Some(if size % 2 == 0 {
        (data[size / 2 - 1] + data[size / 2]) / Decimal::from_i32(2).unwrap()
    } else {
        data[size / 2]
    })
}
