use async_trait::async_trait;
use rust_decimal::Decimal;

#[async_trait]
pub trait TickerDataSource: Sync + Send {
    async fn get_ticker_data(&self) -> TickerData;
}

#[derive(Clone)]
pub struct TickerData {
    pub last_price: Option<Decimal>,
    pub prev_price: Option<Decimal>,
    pub insufficient_data: bool,
    pub errors: Vec<String>,
}