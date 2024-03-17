mod aggregator;
mod binance;
mod coinbase;
mod datasource;
mod goldprice;
mod kraken;
mod yfinance;

pub use aggregator::Aggregator;
pub use binance::BinanceTickerDataSource;
pub use coinbase::CoinbaseTickerDataSource;
pub use datasource::{TickerData, TickerDataSource};
pub use goldprice::GoldpriceTickerDataSource;
pub use kraken::KrakenTickerDataSource;
pub use yfinance::YahooFinanceTickerDataSource;
