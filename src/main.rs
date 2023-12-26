mod datasources;

use anyhow::Result;
use datasources::Aggregator;
use datasources::BinanceTickerDataSource;
use datasources::CoinbaseTickerDataSource;
use datasources::GoldpriceTickerDataSource;
use datasources::KrakenTickerDataSource;
use datasources::TickerData;
use datasources::YahooFinanceTickerDataSource;
use env_logger::Env;
use futures::future::join_all;
use log::error;
use log::warn;
use reqwest::Client;
use rust_decimal::prelude::*;
use teloxide::Bot;
use teloxide::dispatching::Dispatcher;
use teloxide::dispatching::HandlerExt;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::dptree;
use teloxide::macros::BotCommands;
use teloxide::payloads::SendMessageSetters;
use teloxide::requests::Request;
use teloxide::requests::Requester;
use teloxide::types::InlineQuery;
use teloxide::types::InlineQueryResult;
use teloxide::types::InlineQueryResultArticle;
use teloxide::types::InputMessageContent;
use teloxide::types::InputMessageContentText;
use teloxide::types::Message;
use teloxide::types::Update;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use yahoo_finance_api;
use yahoo_finance_api::YahooConnector;

use datasources::TickerDataSource;

struct DataSources {
    btc: Box<dyn TickerDataSource + Sync>,
    eth: Box<dyn TickerDataSource + Sync>,
    sol: Box<dyn TickerDataSource + Sync>,
    gspc: Box<dyn TickerDataSource + Sync>,
    ixic: Box<dyn TickerDataSource + Sync>,
    xau: Box<dyn TickerDataSource + Sync>,
}

struct QueryState {
    tickers: Vec<(String, String, String, bool)>,
    errors: Vec<String>
}

impl DataSources {
    async fn query_all(&self) -> QueryState {
        let results = join_all([
                self.btc.get_ticker_data(),
                self.eth.get_ticker_data(),
                self.sol.get_ticker_data(),
                self.gspc.get_ticker_data(),
                self.ixic.get_ticker_data(),
                self.xau.get_ticker_data(),
                ]).await;
        let tickers = results.iter().zip(["BTC", "ETH", "SOL", "GSPC", "IXIC", "XAU"]).map(|(ticker_data, ticker)| {
            let change = {
                if let TickerData { last_price: Some(last), prev_price: Some(prev), .. } = ticker_data {
                    format!("{:>+.2}%", ((last / prev).to_f64().unwrap() - 1.) * 100.)
                } else {
                    "N/A".to_owned()
                }
            };
            let price = ticker_data.last_price.map(|price| format!("{:>.2}", price)).unwrap_or("N/A".to_owned());
            (ticker.to_owned(), price, change, ticker_data.insufficient_data)
        }).collect();
        let errors = results.into_iter().flat_map(|t| t.errors).collect();

        QueryState { tickers, errors }
    }
}

async fn gen_message(state: &QueryState) -> Result<String> {
    let errmsg = if state.errors.is_empty() {
        String::new()
    } else {
        warn!("{}", state.errors.join("\n"));
        format!(
            "\nError happened while fetching prices:\n{}",
            state.errors.join("\n")
        )
    };
    let width_ticker = state.tickers.iter()
        .map(|s| s.0.len())
        .max()
        .unwrap_or(4);
    let width_price = state.tickers.iter()
        .map(|s| s.1.len())
        .max()
        .unwrap_or(8);
    let width_change = state.tickers.iter()
        .map(|s| s.2.len())
        .max()
        .unwrap_or(8);
    let output = state.tickers.iter().map(|(ticker, price, change, insufficient)| {
        format!("{:<width_ticker$} {:>width_price$} {:>width_change$}", ticker, price, change) + if *insufficient { " *" } else { "" }
    }).collect::<Vec<_>>().join("\n");
    Ok(format!("```\n{}```{}", output, errmsg))
}

async fn get_update(data_sources: &DataSources) -> Result<String> {
    let query_result = data_sources.query_all().await;
    let msgstr = gen_message(&query_result).await?;
    Ok(msgstr)
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    #[command(description = "query coin price")]
    Query,
}


#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let token = env::var("IREINA_TOKEN")?;
    let bot = Bot::new(token);

    let http_client = Arc::new(Client::builder()
        .user_agent("ireina/0.1.0")
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap());

    let yfi = Arc::new(YahooConnector::new());

    let data_sources = DataSources {
        btc: Box::new(Aggregator::new(vec![
            Box::new(BinanceTickerDataSource::new(http_client.clone(), "BTCUSDT".to_string())),
            Box::new(CoinbaseTickerDataSource::new(http_client.clone(), "BTC-USD".to_string())),
            Box::new(KrakenTickerDataSource::new(http_client.clone(), "XXBTZUSD".to_string())),
        ])),
        eth: Box::new(Aggregator::new(vec![
            Box::new(BinanceTickerDataSource::new(http_client.clone(), "ETHUSDT".to_string())),
            Box::new(CoinbaseTickerDataSource::new(http_client.clone(), "ETH-USD".to_string())),
            Box::new(KrakenTickerDataSource::new(http_client.clone(), "XETHZUSD".to_string())),
        ])),
        sol: Box::new(Aggregator::new(vec![
            Box::new(BinanceTickerDataSource::new(http_client.clone(), "SOLUSDT".to_string())),
            Box::new(CoinbaseTickerDataSource::new(http_client.clone(), "SOL-USD".to_string())),
            Box::new(KrakenTickerDataSource::new(http_client.clone(), "SOLUSD".to_string())),
        ])),
        gspc: Box::new(YahooFinanceTickerDataSource::new(yfi.clone(), "^GSPC".to_string())),
        ixic: Box::new(YahooFinanceTickerDataSource::new(yfi.clone(), "^IXIC".to_string())),
        xau: Box::new(Aggregator::new(vec![
            Box::new(YahooFinanceTickerDataSource::new(yfi.clone(), "GC=F".to_string())),
            Box::new(GoldpriceTickerDataSource::new(http_client.clone(), "XAU".to_string(), "USD".to_string())),
        ])),
    };

    let handler = dptree::entry()
        .branch(
            Update::filter_message()
                .filter_command::<Command>()
                .endpoint(command_handler)
        )
        .branch(
            Update::filter_inline_query().endpoint(inline_query_handler)
        );

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .dependencies(dptree::deps![Arc::new(data_sources)])
        .build().dispatch().await;
    Ok(())
}

async fn command_handler(bot: Bot, msg: Message, cmd: Command, data_sources: Arc<DataSources>) -> Result<()> {
    let update = match get_update(&data_sources).await {
        Ok(update) => update,
        Err(e) => {
            error!("get_update: {}", e);
            return Ok(())
        }
    };
    let resp = match cmd {
        Command::Query => bot.send_message(msg.chat.id, update).reply_to_message_id(msg.id).parse_mode(teloxide::types::ParseMode::Markdown).await,
    };
    if let Err(ref e) = resp {
        error!("handle command: {}", e);
    }
    Ok(())
}

async fn inline_query_handler(bot: Bot, q: InlineQuery, data_sources: Arc<DataSources>) -> Result<()> {
    let update = match get_update(&data_sources).await {
        Ok(update) => update,
        Err(e) => {
            error!("get_update: {}", e);
            return Ok(())
        }
    };
    let resp = bot.answer_inline_query(&q.id, vec![
        InlineQueryResult::Article(InlineQueryResultArticle::new("price", "Coin Prices", InputMessageContent::Text(InputMessageContentText::new(update).parse_mode(teloxide::types::ParseMode::Markdown)))),
    ]).send().await;
    if let Err(ref e) = resp {
        error!("handle command: {}", e);
    }
    Ok(())
}