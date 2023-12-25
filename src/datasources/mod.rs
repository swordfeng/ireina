mod datasource;
mod aggregator;
mod coinbase;
mod binance;
mod yfinance;
mod kraken;
mod goldprice;

pub use datasource::{TickerData, TickerDataSource};
pub use aggregator::Aggregator;
pub use coinbase::CoinbaseTickerDataSource;
pub use binance::BinanceTickerDataSource;
pub use kraken::KrakenTickerDataSource;
pub use yfinance::YahooFinanceTickerDataSource;
pub use goldprice::GoldpriceTickerDataSource;