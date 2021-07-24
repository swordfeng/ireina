#![feature(async_closure)]
use anyhow::{anyhow, Result};
use futures::join;
use futures::StreamExt;
use once_cell::sync::Lazy;
use reqwest::Client;
use rust_decimal::prelude::*;
use serde_json::Value as JsonValue;
use std::env;
use std::time::Duration;
use std::time::SystemTime;
use telegram_bot::*;
use tokio_compat_02::FutureExt;
use yahoo_finance_api;
use yahoo_finance_api::YahooConnector;

static CLIENT: Lazy<Client> = Lazy::new(|| {
    Client::builder()
        .user_agent("Ireina 0.1.0")
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap()
});

async fn get_coinbase_price(pair: &str) -> Result<Decimal> {
    let response: JsonValue = CLIENT
        .get(&format!("https://api.coinbase.com/v2/prices/{}/spot", pair))
        .send()
        .await?
        .json()
        .await?;
    if response["errors"] != JsonValue::Null {
        return Err(anyhow!("Coinbase: {}", response["errors"][0]["message"]));
    }
    Ok(Decimal::from_str(
        response["data"]["amount"]
            .as_str()
            .ok_or(anyhow!("Failed to parse Coinbase response"))?,
    )?)
}

async fn get_kraken_price(pair: &str) -> Result<Decimal> {
    let response: JsonValue = CLIENT
        .get(&format!(
            "https://api.kraken.com/0/public/Ticker?pair={}",
            pair
        ))
        .send()
        .await?
        .json()
        .await?;
    if response["error"][0] != JsonValue::Null {
        return Err(anyhow!("Kraken: {}", response["error"][0]));
    }
    Ok(Decimal::from_str(
        response["result"]
            .as_object()
            .and_then(|m| m.values().next())
            .and_then(|p| p["c"][0].as_str())
            .ok_or(anyhow!("Failed to parse Kraken response"))?,
    )?)
}

async fn get_gemini_price(pair: &str) -> Result<Decimal> {
    let response: JsonValue = CLIENT
        .get(&format!("https://api.gemini.com/v1/pubticker/{}", pair))
        .send()
        .await?
        .json()
        .await?;
    if response["result"].as_str() == Some("error") {
        return Err(anyhow!("Gemini: {}", response["message"]));
    }
    Ok(Decimal::from_str(
        response["last"]
            .as_str()
            .ok_or(anyhow!("Failed to parse Kraken response"))?,
    )?)
}

async fn get_binance_price(pair: &str) -> Result<Decimal> {
    let response: JsonValue = CLIENT
        .get(&format!(
            "https://api.binance.com/api/v3/trades?symbol={}&limit=1",
            pair
        ))
        .send()
        .await?
        .json()
        .await?;
    if response["code"].is_i64() {
        return Err(anyhow!("Binance: {}", response["msg"]));
    }
    Ok(Decimal::from_str(
        response[0]["price"]
            .as_str()
            .ok_or(anyhow!("Failed to parse Kraken response"))?,
    )?)
}

fn median_price_string(data: &mut [Decimal]) -> String {
    data.sort();
    let size = data.len();
    if size == 0 {
        return "ERROR".to_owned();
    }
    (if size % 2 == 0 {
        (data[size / 2 - 1] + data[size / 2]) / Decimal::from_i32(2).unwrap()
    } else {
        data[size / 2]
    })
    .round_dp_with_strategy(2, RoundingStrategy::BankersRounding)
    .to_string()
}

async fn get_btc_price(errors: &mut Vec<String>) -> String {
    let (btc1, btc2, btc3, btc4) = join!(
        get_coinbase_price("BTC-USD"),
        get_kraken_price("BTCUSD"),
        get_gemini_price("BTCUSD"),
        get_binance_price("BTCUSDT"),
    );
    let mut prices = vec![];
    for b in [btc1, btc2, btc3, btc4].iter() {
        match b {
            Ok(price) => prices.push(price.clone()),
            Err(e) => errors.push(e.to_string()),
        }
    }
    median_price_string(&mut prices)
}

async fn get_eth_price(errors: &mut Vec<String>) -> String {
    let (eth1, eth2, eth3, eth4) = join!(
        get_coinbase_price("ETH-USD"),
        get_kraken_price("ETHUSD"),
        get_gemini_price("ETHUSD"),
        get_binance_price("ETHUSDT"),
    );
    let mut prices = vec![];
    for b in [eth1, eth2, eth3, eth4].iter() {
        match b {
            Ok(price) => prices.push(price.clone()),
            Err(e) => errors.push(e.to_string()),
        }
    }
    median_price_string(&mut prices)
}

async fn get_dot_price(errors: &mut Vec<String>) -> String {
    let (dot1, dot2, dot3) = join!(
        get_kraken_price("DOTUSD"),
        get_coinbase_price("DOT-USD"),
        get_binance_price("DOTUSDT")
    );
    let mut prices = vec![];
    for b in [dot1, dot2, dot3].iter() {
        match b {
            Ok(price) => prices.push(price.clone()),
            Err(e) => errors.push(e.to_string()),
        }
    }
    median_price_string(&mut prices)
}

struct State {
    api: Api,
    yfi: yahoo_finance_api::YahooConnector,
    last_update: SystemTime,
    btc: String,
    eth: String,
    dot: String,
    gspc: String,
    ixic: String,
    btc6h: String,
    eth6h: String,
    dot6h: String,
    gspc6h: String,
    ixic6h: String,
}

async fn yfi_get(
    yfi: &yahoo_finance_api::YahooConnector,
    symbol: &str,
) -> Result<(String, String)> {
    let quotes = if symbol.ends_with("-USD") {
        yfi.get_quote_range(symbol, "1h", "2d")
    } else {
        yfi.get_quote_range(symbol, "1d", "2d")
    }
    .await?
    .quotes()?
    .into_iter()
    .rev()
    .collect::<Vec<_>>();
    let prev_price = if symbol.ends_with("-USD") {
        if quotes.len() >= 25 {
            quotes[24].close
        } else {
            0.
        }
    } else {
        if quotes.len() >= 2 {
            quotes[1].adjclose
        } else {
            0.
        }
    };
    if quotes.len() == 0 {
        Ok(("N/A".to_owned(), "N/A".to_owned()))
    } else if prev_price == 0. {
        Ok((format!("{:.2}", quotes[0].close), "N/A".to_owned()))
    } else {
        let change = if symbol.ends_with("-USD") {
            (quotes[0].close / prev_price - 1.) * 100.
        } else {
            (quotes[0].adjclose / prev_price - 1.) * 100.
        };
        Ok((
            format!("{:.2}", quotes[0].close),
            format!("{:>+.2}%", change),
        ))
    }
}

async fn yfi_get_wrapped(
    yfi: &yahoo_finance_api::YahooConnector,
    symbol: &str,
    errors: &mut Vec<String>,
) -> (String, String) {
    yfi_get(yfi, symbol).await.unwrap_or_else(|err| {
        errors.push(err.to_string());
        ("ERROR".to_owned(), String::new())
    })
}

async fn update_state(state: &mut State, errors: &mut Vec<String>) {
    state.btc = get_btc_price(errors).await;
    state.eth = get_eth_price(errors).await;
    state.dot = get_dot_price(errors).await;

    let (_, btc6h) = yfi_get_wrapped(&state.yfi, "BTC-USD", errors).await;
    state.btc6h = btc6h;
    let (_, eth6h) = yfi_get_wrapped(&state.yfi, "ETH-USD", errors).await;
    state.eth6h = eth6h;
    let (_, dot6h) = yfi_get_wrapped(&state.yfi, "DOT1-USD", errors).await;
    state.dot6h = dot6h;

    let (gspc, gspc6h) = yfi_get_wrapped(&state.yfi, "^GSPC", errors).await;
    state.gspc = gspc;
    state.gspc6h = gspc6h;
    let (ixic, ixic6h) = yfi_get_wrapped(&state.yfi, "^IXIC", errors).await;
    state.ixic = ixic;
    state.ixic6h = ixic6h;

    state.last_update = SystemTime::now();
}

async fn gen_message(state: &mut State) -> Result<String> {
    let mut errors = vec![];
    if state.last_update.elapsed()? > Duration::from_secs(3) {
        update_state(state, &mut errors).await;
    }
    let errmsg = if errors.is_empty() {
        String::new()
    } else {
        format!(
            "\nError happened while fetching prices:\n{}",
            errors.join("\n")
        )
    };
    let width = [&state.btc, &state.eth, &state.dot, &state.gspc, &state.ixic]
        .iter()
        .map(|s| s.len())
        .max()
        .unwrap_or(8);
    let width2 = [
        &state.btc6h,
        &state.eth6h,
        &state.dot6h,
        &state.gspc6h,
        &state.ixic6h,
    ]
    .iter()
    .map(|s| s.len())
    .max()
    .unwrap_or(8);
    Ok(format!(
        "```\nBTC  {:>width$} {:>width2$}\nETH  {:>width$} {:>width2$}\nDOT  {:>width$} {:>width2$}\n\nGSPC {:>width$} {:>width2$}\nIXIC {:>width$} {:>width2$}```{}",
        state.btc,
        state.btc6h,
        state.eth,
        state.eth6h,
        state.dot,
        state.dot6h,
        state.gspc,
        state.gspc6h,
        state.ixic,
        state.ixic6h,
        errmsg,
        width = width,
        width2 = width2,
    ))
}

async fn handle_update(state: &mut State, update: Update) -> Result<()> {
    if let UpdateKind::Message(message) = update.kind {
        if let MessageKind::Text { ref data, .. } = message.kind {
            if data.starts_with("/query") {
                let msgstr = gen_message(state).await?;
                state
                    .api
                    .send(SendMessage::new(message.chat, msgstr).parse_mode(ParseMode::Markdown))
                    .compat()
                    .await?;
            }
        }
    } else if let UpdateKind::InlineQuery(query) = update.kind {
        let msgstr = gen_message(state).await?;
        let answer = AnswerInlineQueryTimeout {
            inline_query_id: query.id,
            results: vec![InlineQueryResultArticle::new(
                "price",
                "Coin Prices",
                InputTextMessageContent {
                    message_text: msgstr,
                    parse_mode: Some(ParseMode::Markdown),
                    disable_web_page_preview: true,
                },
            )
            .into()],
            cache_time: 5,
        };
        state.api.send(answer).compat().await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let token = env::var("IREINA_TOKEN")?;

    let mut state = State {
        api: Api::new(token),
        yfi: YahooConnector::new(),
        last_update: SystemTime::UNIX_EPOCH,
        btc: String::new(),
        eth: String::new(),
        dot: String::new(),
        gspc: String::new(),
        ixic: String::new(),
        btc6h: String::new(),
        eth6h: String::new(),
        dot6h: String::new(),
        gspc6h: String::new(),
        ixic6h: String::new(),
    };

    let mut stream = state.api.stream();
    while let Some(update) = stream.next().compat().await {
        let update = match update {
            Ok(u) => u,
            Err(e) => {
                eprintln!("{}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        if let Err(e) = handle_update(&mut state, update).await {
            eprintln!("{}", e);
        }
    }
    Ok(())
}

#[derive(serde::Serialize, Debug)]
pub struct AnswerInlineQueryTimeout {
    inline_query_id: InlineQueryId,
    results: Vec<InlineQueryResult>,
    cache_time: i32,
}

impl Request for AnswerInlineQueryTimeout {
    type Type = JsonRequestType<Self>;
    type Response = JsonTrueToUnitResponse;

    fn serialize(&self) -> Result<HttpRequest, telegram_bot::types::Error> {
        Self::Type::serialize(RequestUrl::method("answerInlineQuery"), self)
    }
}
