//! Yahoo Finance API client for fetching stock and cryptocurrency quotes.
//!
//! Because checking your portfolio every 5 seconds is totally healthy behavior.

use crate::models::{MarketState, Quote, QuoteBatch, QuoteType};
use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use futures::stream::{FuturesUnordered, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use urlencoding::encode;

/// The v8 chart API endpoint - the one that still works (for now).
const YAHOO_CHART_URL: &str = "https://query1.finance.yahoo.com/v8/finance/chart";

/// Financial Modeling Prep API for company fundamentals
const FMP_API_URL: &str = "https://financialmodelingprep.com/api/v3/profile";

/// Pretending to be a real browser because Yahoo has trust issues.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Free FMP API key (public tier) - limited but works for market cap lookups
fn get_fmp_api_key() -> String {
    std::env::var("FMP_API_KEY").unwrap_or_else(|_| "demo".to_string())
}

/// Yahoo Finance API client.
/// Your gateway to financial anxiety delivered in JSON format.
#[derive(Clone)]
pub struct YahooFinanceClient {
    pub client: Client,
    timeout: Duration,
    max_concurrency: usize,
}

/// Validate that a symbol has acceptable format before API call.
/// Prevents wasting API quota on junk inputs.
fn is_valid_symbol(s: &str) -> bool {
    !s.is_empty() && s.len() <= 10 && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '^')
}

impl YahooFinanceClient {
    /// Create a new Yahoo Finance client.
    pub fn new(timeout_secs: u64) -> Result<Self> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            timeout: Duration::from_secs(timeout_secs),
            max_concurrency: 12,
        })
    }

    pub fn with_max_concurrency(mut self, max: usize) -> Self {
        self.max_concurrency = max.max(1);
        self
    }

    /// Fetch quotes for multiple symbols using parallel requests.
    /// Yahoo's v8 chart API only supports one symbol at a time, so we parallelize.
    /// Market cap fetches are also parallelized to avoid sequential bottleneck.
    pub async fn get_quotes(&self, symbols: &[String]) -> Result<QuoteBatch> {
        if symbols.is_empty() {
            return Ok(QuoteBatch::default());
        }

        // Separate valid and invalid symbols
        let valid_symbols: Vec<_> = symbols.iter().filter(|s| is_valid_symbol(s)).cloned().collect();
        let invalid_symbols: Vec<_> = symbols.iter().filter(|s| !is_valid_symbol(s)).cloned().collect();

        let sem = Arc::new(Semaphore::new(self.max_concurrency));
        let mut futs = FuturesUnordered::new();

        for symbol in valid_symbols {
            let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
            let sym = symbol.clone();
            let client_clone = self.clone();
            
            futs.push(async move {
                let _permit = permit;
                let sym_clone = sym.clone();
                
                // Fetch quote
                let mut result = match client_clone.fetch_single_quote(&sym).await {
                    Ok(q) => Ok(q),
                    Err(e) => Err(e),
                };
                
                // If quote succeeded, try to fetch market cap in parallel (no sequential block)
                if let Ok(ref mut q) = result {
                    if q.market_cap == Some(0) {
                        // Market cap fetch happens here, still within the parallelized task
                        let http_client = client_clone.client.clone();
                        if let Some(cap) = fetch_market_cap(&http_client, &sym).await {
                            q.market_cap = Some(cap);
                        }
                    }
                }
                
                (sym_clone, result)
            });
        }

        let mut batch = QuoteBatch::default();
        
        // Add invalid symbols to failures
        for invalid in invalid_symbols {
            batch.failures.push((invalid, "Invalid symbol format".to_string()));
        }
        
        // Process valid symbol responses - no sequential blocking
        while let Some((sym, res)) = futs.next().await {
            match res {
                Ok(q) => batch.quotes.push(q),
                Err(e) => batch.failures.push((sym, format!("{:#}", e))),
            }
        }

        Ok(batch)
    }

    /// Fetch a single quote from the v8 chart API.
    pub async fn fetch_single_quote(&self, symbol: &str) -> Result<Quote> {
        // Symbol goes in the path, not as a query parameter.
        // Must be encoded to support symbols like ^VIX.
        let sym = encode(symbol);
        let url = format!("{}/{}?interval=1d&range=1d", YAHOO_CHART_URL, sym);

        let response = self
            .client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await
            .with_context(|| format!("Failed to fetch quote for {}", symbol))?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Yahoo Finance API returned error for {}: {}",
                symbol,
                response.status()
            );
        }

        let data: ChartResponse = response
            .json()
            .await
            .with_context(|| format!("Failed to parse response for {}", symbol))?;

        // Check for API errors
        if let Some(error) = data.chart.error {
            anyhow::bail!("Yahoo Finance error for {}: {}", symbol, error.description);
        }

        let result = data
            .chart
            .result
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| {
                if let Some(error) = data.chart.error {
                    anyhow::anyhow!("Yahoo Finance error for {}: {}", symbol, error.description)
                } else {
                    anyhow::anyhow!("No data returned for {}. Server may not be responding or symbol may be invalid.", symbol)
                }
            })?;

        Ok(result.into_quote())
    }

    /// Fetch a single quote.
    /// For when you only need to be disappointed by one stock at a time.
    #[allow(dead_code)] // Reserved for future regret-checking functionality
    pub async fn get_quote(&self, symbol: &str) -> Result<Quote> {
        self.fetch_single_quote(symbol).await
    }
}

impl Default for YahooFinanceClient {
    fn default() -> Self {
        Self::new(10).expect("Failed to create default client")
    }
}

// Yahoo Finance v8 Chart API response structures

#[derive(Debug, Deserialize)]
struct ChartResponse {
    chart: ChartData,
}

#[derive(Debug, Deserialize)]
struct ChartData {
    result: Option<Vec<ChartResult>>,
    error: Option<ChartError>,
}

#[derive(Debug, Deserialize)]
struct ChartError {
    #[allow(dead_code)]
    code: String,
    description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartResult {
    meta: ChartMeta,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChartMeta {
    symbol: String,
    #[serde(default)]
    short_name: Option<String>,
    #[serde(default)]
    long_name: Option<String>,
    #[serde(default)]
    regular_market_price: Option<f64>,
    #[serde(default)]
    chart_previous_close: Option<f64>,
    #[serde(default)]
    previous_close: Option<f64>,
    #[serde(default)]
    regular_market_day_high: Option<f64>,
    #[serde(default)]
    regular_market_day_low: Option<f64>,
    #[serde(default)]
    fifty_two_week_high: Option<f64>,
    #[serde(default)]
    fifty_two_week_low: Option<f64>,
    #[serde(default)]
    regular_market_volume: Option<u64>,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    exchange_name: Option<String>,
    #[serde(default)]
    instrument_type: Option<String>,
    #[serde(default)]
    regular_market_time: Option<i64>,
    #[serde(default)]
    market_state: Option<String>,
}

impl ChartResult {
    fn into_quote(self) -> Quote {
        let meta = self.meta;
        let prev_close = meta
            .chart_previous_close
            .or(meta.previous_close)
            .unwrap_or(0.0);
        let price = meta.regular_market_price.unwrap_or(0.0);
        let change = price - prev_close;
        let change_percent = if prev_close > 0.0 {
            (change / prev_close) * 100.0
        } else {
            0.0
        };

        Quote {
            symbol: meta.symbol,
            name: meta
                .short_name
                .or(meta.long_name)
                .unwrap_or_else(|| "Unknown".to_string()),
            price,
            change,
            change_percent,
            previous_close: prev_close,
            open: 0.0, // Not available in chart API meta
            day_high: meta.regular_market_day_high.unwrap_or(0.0),
            day_low: meta.regular_market_day_low.unwrap_or(0.0),
            year_high: meta.fifty_two_week_high.unwrap_or(0.0),
            year_low: meta.fifty_two_week_low.unwrap_or(0.0),
            volume: meta.regular_market_volume.unwrap_or(0),
            avg_volume: 0, // Not available in chart API meta
            market_cap: Some(0), // Not available in chart API meta; display shows 0 instead of blank
            currency: meta.currency.unwrap_or_else(|| "USD".to_string()),
            exchange: meta.exchange_name.unwrap_or_default(),
            quote_type: parse_quote_type(meta.instrument_type.as_deref()),
            market_state: parse_market_state(meta.market_state.as_deref()),
            timestamp: meta
                .regular_market_time
                .and_then(|t| Utc.timestamp_opt(t, 0).single())
                .unwrap_or_else(Utc::now),
        }
    }
}

fn parse_quote_type(s: Option<&str>) -> QuoteType {
    match s {
        Some("EQUITY") => QuoteType::Equity,
        Some("CRYPTOCURRENCY") => QuoteType::Cryptocurrency,
        Some("ETF") => QuoteType::Etf,
        Some("MUTUALFUND") => QuoteType::MutualFund,
        Some("INDEX") => QuoteType::Index,
        Some("CURRENCY") => QuoteType::Currency,
        Some("FUTURE") => QuoteType::Future,
        Some("OPTION") => QuoteType::Option,
        _ => QuoteType::Equity,
    }
}

/// Parse market state from string representation.
fn parse_market_state(s: Option<&str>) -> MarketState {
    match s {
        Some("PRE") => MarketState::Pre,
        Some("REGULAR") => MarketState::Regular,
        Some("POST") => MarketState::Post,
        Some("CLOSED") => MarketState::Closed,
        _ => MarketState::Closed,
    }
}

/// Symbol shortcuts for common cryptocurrencies.
/// Because typing "-USD" is too much work for crypto bros.
pub fn expand_symbol(symbol: &str) -> String {
    // Handle shorthand crypto symbols like "BTC.X" -> "BTC-USD"
    // The .X suffix is like X marks the spot, but for losing money
    if let Some(base) = symbol.strip_suffix(".X") {
        return format!("{}-USD", base);
    }

    // Common crypto shortcuts
    let shortcuts: HashMap<&str, &str> = [
        ("BTC", "BTC-USD"),
        ("ETH", "ETH-USD"),
        ("SOL", "SOL-USD"),
        ("DOGE", "DOGE-USD"),
        ("XRP", "XRP-USD"),
        ("ADA", "ADA-USD"),
        ("DOT", "DOT-USD"),
        ("MATIC", "MATIC-USD"),
        ("LINK", "LINK-USD"),
        ("UNI", "UNI-USD"),
        ("AVAX", "AVAX-USD"),
        ("ATOM", "ATOM-USD"),
        ("LTC", "LTC-USD"),
    ]
    .into_iter()
    .collect();

    // Only expand if it looks like a crypto symbol (all caps, short)
    if symbol.len() <= 5 && symbol.chars().all(|c| c.is_ascii_uppercase()) {
        if let Some(expanded) = shortcuts.get(symbol) {
            return expanded.to_string();
        }
    }

    symbol.to_string()
}

/// FMP Company Profile response for getting market cap
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FmpProfile {
    symbol: Option<String>,
    market_cap: Option<u64>,
}

/// Fetch market cap from Financial Modeling Prep API
/// Falls back gracefully if unavailable
pub async fn fetch_market_cap(client: &Client, symbol: &str) -> Option<u64> {
    let api_key = get_fmp_api_key();
    let url = format!(
        "{}/{}?apikey={}",
        FMP_API_URL, symbol, api_key
    );

    match tokio::time::timeout(
        Duration::from_secs(5),
        client.get(&url).send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            match response.json::<FmpProfile>().await {
                Ok(profile) => profile.market_cap,
                Err(_) => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_symbol_crypto_shorthand() {
        assert_eq!(expand_symbol("BTC.X"), "BTC-USD");
        assert_eq!(expand_symbol("ETH.X"), "ETH-USD");
    }

    #[test]
    fn test_expand_symbol_crypto_shortcuts() {
        assert_eq!(expand_symbol("BTC"), "BTC-USD");
        assert_eq!(expand_symbol("ETH"), "ETH-USD");
    }

    #[test]
    fn test_expand_symbol_stock() {
        assert_eq!(expand_symbol("AAPL"), "AAPL");
        assert_eq!(expand_symbol("GOOGL"), "GOOGL");
    }

    #[test]
    fn test_is_valid_symbol_valid() {
        assert!(is_valid_symbol("AAPL"));
        assert!(is_valid_symbol("BTC-USD"));
        assert!(is_valid_symbol("ETH.X"));
        assert!(is_valid_symbol("^VIX"));
        assert!(is_valid_symbol("GOOGL"));
    }

    #[test]
    fn test_is_valid_symbol_invalid() {
        assert!(!is_valid_symbol("")); // Empty
        assert!(!is_valid_symbol("AAAABBBBCCDD")); // Too long (>10)
        assert!(!is_valid_symbol("AAPL GOOGL")); // Spaces
        assert!(!is_valid_symbol("AAPL\n")); // Newline
        assert!(!is_valid_symbol("AAA@BBB")); // Invalid character
    }
}
