//! Data export functionality for accessibility and integration.
//!
//! Provides CSV, JSON, and plain text export formats.
//! Useful for screen readers, data analysis, and integration with other tools.

use crate::models::Quote;

/// Export format type
#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Text,
    Csv,
    Json,
}

/// Export quotes in the specified format.
pub fn export_quotes(quotes: &[Quote], format: ExportFormat) -> String {
    match format {
        ExportFormat::Text => export_text(quotes),
        ExportFormat::Csv => export_csv(quotes),
        ExportFormat::Json => export_json(quotes),
    }
}

/// Export as plain text (screen reader friendly).
fn export_text(quotes: &[Quote]) -> String {
    let mut output = String::new();
    
    // Header
    output.push_str("STONKTOP DATA EXPORT\n");
    output.push_str("====================\n\n");
    
    for quote in quotes {
        output.push_str(&format!("Symbol: {}\n", quote.symbol));
        output.push_str(&format!("Name: {}\n", quote.name));
        output.push_str(&format!("Price: ${:.2}\n", quote.price));
        output.push_str(&format!("Change: {:+.2}\n", quote.change));
        output.push_str(&format!("Change %: {:+.2}%\n", quote.change_percent));
        output.push_str(&format!("Volume: {}\n", format_volume(quote.volume)));
        let market_cap_str = quote.market_cap.map(|mc| format_market_cap(mc as f64)).unwrap_or_else(|| "N/A".to_string());
        output.push_str(&format!("Market Cap: {}\n", market_cap_str));
        output.push_str("\n");
    }
    
    output
}

/// Export as CSV (comma-separated values).
fn export_csv(quotes: &[Quote]) -> String {
    let mut output = String::new();
    
    // CSV Header
    output.push_str("Symbol,Name,Price,Change,Change%,Volume,MarketCap\n");
    
    // CSV Data
    for quote in quotes {
        let market_cap_str = quote.market_cap.map(|mc| mc.to_string()).unwrap_or_else(|| "N/A".to_string());
        output.push_str(&format!(
            "\"{}\",\"{}\",{:.2},{:.2},{:.2},{},{}\n",
            quote.symbol,
            quote.name,
            quote.price,
            quote.change,
            quote.change_percent,
            quote.volume,
            market_cap_str,
        ));
    }
    
    output
}

/// Export as JSON.
fn export_json(quotes: &[Quote]) -> String {
    let mut output = String::from("[\n");
    
    for (i, quote) in quotes.iter().enumerate() {
        output.push_str("  {\n");
        output.push_str(&format!("    \"symbol\": \"{}\",\n", quote.symbol));
        output.push_str(&format!("    \"name\": \"{}\",\n", quote.name));
        output.push_str(&format!("    \"price\": {:.2},\n", quote.price));
        output.push_str(&format!("    \"change\": {:.2},\n", quote.change));
        output.push_str(&format!("    \"changePercent\": {:.2},\n", quote.change_percent));
        output.push_str(&format!("    \"volume\": {},\n", quote.volume));
        let market_cap_str = quote.market_cap.map(|mc| mc.to_string()).unwrap_or_else(|| "null".to_string());
        output.push_str(&format!("    \"marketCap\": {}\n", market_cap_str));
        output.push_str("  }");
        
        if i < quotes.len() - 1 {
            output.push(',');
        }
        output.push('\n');
    }
    
    output.push_str("]\n");
    output
}

/// Format volume with K/M/B suffixes
fn format_volume(volume: u64) -> String {
    if volume == 0 {
        return "0".to_string();
    }
    
    if volume >= 1_000_000_000 {
        format!("{:.1}B", volume as f64 / 1_000_000_000.0)
    } else if volume >= 1_000_000 {
        format!("{:.1}M", volume as f64 / 1_000_000.0)
    } else if volume >= 1_000 {
        format!("{:.1}K", volume as f64 / 1_000.0)
    } else {
        volume.to_string()
    }
}

/// Format market cap with K/M/B/T suffixes
fn format_market_cap(market_cap: f64) -> String {
    if market_cap == 0.0 {
        return "N/A".to_string();
    }
    
    if market_cap >= 1_000_000_000_000.0 {
        format!("${:.2}T", market_cap / 1_000_000_000_000.0)
    } else if market_cap >= 1_000_000_000.0 {
        format!("${:.2}B", market_cap / 1_000_000_000.0)
    } else if market_cap >= 1_000_000.0 {
        format!("${:.2}M", market_cap / 1_000_000.0)
    } else if market_cap >= 1_000.0 {
        format!("${:.2}K", market_cap / 1_000.0)
    } else {
        format!("${:.2}", market_cap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_quote(symbol: &str, price: f64) -> Quote {
        use chrono::Utc;
        Quote {
            symbol: symbol.to_string(),
            name: format!("{} Inc.", symbol),
            price,
            change: 1.0,
            change_percent: 1.5,
            previous_close: price - 1.0,
            open: price - 0.5,
            day_high: price + 1.0,
            day_low: price - 2.0,
            year_high: price + 100.0,
            year_low: price - 50.0,
            volume: 1_000_000,
            avg_volume: 2_000_000,
            market_cap: Some(100_000_000_000),
            currency: "USD".to_string(),
            exchange: "NASDAQ".to_string(),
            quote_type: crate::models::QuoteType::Equity,
            market_state: crate::models::MarketState::Regular,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_export_csv() {
        let quotes = vec![create_test_quote("AAPL", 150.0)];
        let csv = export_csv(&quotes);
        assert!(csv.contains("Symbol,Name,Price"));
        assert!(csv.contains("AAPL"));
    }

    #[test]
    fn test_export_json() {
        let quotes = vec![create_test_quote("AAPL", 150.0)];
        let json = export_json(&quotes);
        assert!(json.contains("\"symbol\": \"AAPL\""));
        assert!(json.contains("\"price\": 150.00"));
    }

    #[test]
    fn test_format_volume() {
        assert_eq!(format_volume(0), "0");
        assert_eq!(format_volume(1_000), "1.0K");
        assert_eq!(format_volume(1_000_000), "1.0M");
        assert_eq!(format_volume(1_000_000_000), "1.0B");
    }

    #[test]
    fn test_format_market_cap() {
        assert_eq!(format_market_cap(0.0), "N/A");
        assert!(format_market_cap(100_000.0).contains("$"));
        assert_eq!(format_market_cap(1_000_000_000_000.0), "$1.00T");
    }
}
