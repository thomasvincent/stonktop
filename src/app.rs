//! Application state and logic.
//!
//! Where we keep track of your hopes, dreams, and unrealized losses.

use crate::api::{expand_symbol, YahooFinanceClient};
use crate::cli::Args;
use crate::config::Config;
use crate::models::{Holding, Quote, SortDirection, SortOrder};
use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Application state.
/// Think of it as your financial life, but with better error handling.
pub struct App {
    /// Current quotes
    pub quotes: Vec<Quote>,
    /// Last batch failures (symbol, error)
    pub failures: Vec<(String, String)>,
    /// Holdings/portfolio
    pub holdings: HashMap<String, Holding>,
    /// Symbols being watched
    pub symbols: Vec<String>,
    /// API client
    client: YahooFinanceClient,
    /// Last refresh time
    pub last_refresh: Option<Instant>,
    /// Last refresh attempt time (updates even on failure to prevent hammering)
    pub last_refresh_attempt: Option<Instant>,
    /// Refresh interval
    pub refresh_interval: Duration,
    /// Current sort order
    pub sort_order: SortOrder,
    /// Sort direction
    pub sort_direction: SortDirection,
    /// Current iteration count
    pub iteration: u64,
    /// Maximum iterations (0 = infinite)
    pub max_iterations: u64,
    /// Is the app running
    pub running: bool,
    /// Error message to display
    pub error: Option<String>,
    /// Selected row index
    pub selected: usize,
    /// Scroll offset for when you have more regrets than fit on screen
    pub scroll_offset: usize,
    /// Show help overlay
    pub show_help: bool,
    /// Show holdings view
    pub show_holdings: bool,
    /// Show fundamentals
    pub show_fundamentals: bool,
    /// Show portfolio dashboard
    pub show_dashboard: bool,
    /// Show detail view for selected stock
    pub show_detail_view: bool,
    /// Batch mode (non-interactive)
    pub batch_mode: bool,
    /// Secure mode (no interactive commands)
    pub secure_mode: bool,
    /// Active group index
    pub active_group: usize,
    /// Group names
    pub groups: Vec<String>,
    /// Group map (name -> symbols)
    pub group_map: HashMap<String, Vec<String>>,
    /// Verbose mode - for when you want MORE numbers to stress about
    #[allow(dead_code)] // TODO: Add more verbosity, because anxiety needs details
    pub verbose: bool,
    /// Search query (active search mode if Some)
    pub search_query: Option<String>,
    /// Filtered quotes for search results
    pub filtered_quotes: Vec<Quote>,
    /// Search input buffer (while user is typing)
    pub search_input: String,
    /// Currently in search mode (accepting input)
    pub search_mode: bool,
    /// Cache: symbol -> (Quote, fetch_time)
    pub quote_cache: HashMap<String, (Quote, Instant)>,
    /// Cache duration (30 seconds)
    pub cache_duration: Duration,
    /// Price alerts: symbol -> Vec<(condition, price)>
    pub alerts: HashMap<String, Vec<(AlertCondition, f64)>>,
    /// Triggered alerts: (symbol, condition, price, current_price)
    pub triggered_alerts: Vec<(String, AlertCondition, f64, f64)>,
    /// Alert setup mode: (symbol, condition_mode, price_input)
    pub alert_setup_mode: Option<(String, AlertSetupMode)>,
    /// Fetch time for each quote (for data freshness)
    pub quote_fetch_times: HashMap<String, Instant>,
    /// Historical prices for sparklines (last 20 closes)
    pub price_history: HashMap<String, Vec<f64>>,
    /// Technical indicators cache (RSI, MACD, SMA)
    pub indicators_cache: HashMap<String, (f64, f64, f64)>,
    /// High contrast mode for accessibility
    pub high_contrast: bool,
    /// Enable audible alerts for price conditions
    pub audio_alerts: bool,
}

#[derive(Debug, Clone)]
pub enum AlertSetupMode {
    SelectCondition(usize), // 0=Above, 1=Below, 2=Equal
    EnterPrice(AlertCondition, String),
}

/// Price alert condition
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlertCondition {
    Above,
    Below,
    Equal,
}

impl Default for App {
    fn default() -> Self {
        Self {
            quotes: Vec::new(),
            failures: Vec::new(),
            holdings: HashMap::new(),
            symbols: Vec::new(),
            client: YahooFinanceClient::default(),
            last_refresh: None,
            last_refresh_attempt: None,
            refresh_interval: Duration::from_secs(5),
            sort_order: SortOrder::ChangePercent,
            sort_direction: SortDirection::Descending,
            iteration: 0,
            max_iterations: 0,
            running: true,
            error: None,
            selected: 0,
            scroll_offset: 0,
            show_help: false,
            show_holdings: false,
            show_fundamentals: false,
            show_dashboard: false,
            show_detail_view: false,
            batch_mode: false,
            secure_mode: false,
            active_group: 0,
            groups: Vec::new(),
            group_map: HashMap::new(),
            verbose: false,
            search_query: None,
            filtered_quotes: Vec::new(),
            search_input: String::new(),
            search_mode: false,
            quote_cache: HashMap::new(),
            cache_duration: Duration::from_secs(30),
            alerts: HashMap::new(),
            triggered_alerts: Vec::new(),
            alert_setup_mode: None,
            quote_fetch_times: HashMap::new(),
            price_history: HashMap::new(),
            indicators_cache: HashMap::new(),
            high_contrast: false,
            audio_alerts: false,
        }
    }
}

impl App {
    /// Create a new application from CLI args and config.
    pub fn new(args: &Args, config: &Config) -> Result<Self> {
        // Merge symbols from CLI args and config
        let mut symbols: Vec<String> = if let Some(ref cli_symbols) = args.symbols {
            let mut merged = cli_symbols.clone();
            merged.extend(config.all_symbols());
            merged
        } else {
            config.all_symbols()
        };

        // Expand symbol shortcuts
        symbols = symbols.into_iter().map(|s| expand_symbol(&s)).collect();

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        symbols.retain(|s| seen.insert(s.clone()));

        // Build holdings map
        let holdings: HashMap<String, Holding> = config
            .get_holdings()
            .into_iter()
            .map(|h| (expand_symbol(&h.symbol), h))
            .collect();

        // Get groups (stable sort for predictable UI)
        let mut groups: Vec<String> = config.groups.keys().cloned().collect();
        groups.sort();
        let group_map = config.groups.clone();

        let client = YahooFinanceClient::new(args.timeout)?.with_max_concurrency(12);

        let mut result = Self {
            quotes: Vec::new(),
            failures: Vec::new(),
            holdings,
            symbols,
            client,
            last_refresh: None,
            last_refresh_attempt: None,
            refresh_interval: Duration::from_secs_f64(args.delay),
            sort_order: args.sort.into(),
            sort_direction: if args.reverse {
                SortDirection::Ascending
            } else {
                SortDirection::Descending
            },
            iteration: 0,
            max_iterations: args.iterations,
            running: true,
            error: None,
            selected: 0,
            scroll_offset: 0,
            show_help: false,
            show_holdings: args.holdings || config.display.show_holdings,
            show_fundamentals: config.display.show_fundamentals,
            batch_mode: args.batch,
            secure_mode: args.secure,
            active_group: 0,
            groups,
            group_map,
            verbose: args.verbose,
            search_query: None,
            filtered_quotes: Vec::new(),
            search_input: String::new(),
            search_mode: false,
            quote_cache: HashMap::new(),
            cache_duration: Duration::from_secs(30),
            alerts: HashMap::new(),
            triggered_alerts: Vec::new(),
            alert_setup_mode: None,
            quote_fetch_times: HashMap::new(),
            show_dashboard: false,
            show_detail_view: false,
            price_history: HashMap::new(),
            indicators_cache: HashMap::new(),
            high_contrast: args.high_contrast,
            audio_alerts: args.audio_alerts,
        };

        // Load persisted alerts from config
        result.load_alerts_from_config(config);
        Ok(result)
    }

    /// Load alerts from configuration file.
    fn load_alerts_from_config(&mut self, config: &Config) {
        for (symbol, alert_configs) in &config.alerts {
            let expanded_symbol = expand_symbol(symbol);
            let mut alerts_for_symbol = Vec::new();

            for alert_config in alert_configs {
                let condition = match alert_config.condition.to_lowercase().as_str() {
                    "above" => AlertCondition::Above,
                    "below" => AlertCondition::Below,
                    "equal" => AlertCondition::Equal,
                    _ => continue, // Skip invalid conditions
                };
                alerts_for_symbol.push((condition, alert_config.price));
            }

            if !alerts_for_symbol.is_empty() {
                self.alerts.insert(expanded_symbol, alerts_for_symbol);
            }
        }
    }

    /// Check if refresh is needed.
    pub fn needs_refresh(&self) -> bool {
        match self.last_refresh {
            None => true,
            Some(last) => last.elapsed() >= self.refresh_interval,
        }
    }

    /// Return the symbol universe for the current UI group selection.
    /// If no groups are configured, returns the full symbol list.
    pub fn active_symbols(&self) -> Vec<String> {
        // If there are no groups, return full list.
        if self.groups.is_empty() {
            return self.symbols.clone();
        }

        let group_name = match self.groups.get(self.active_group) {
            Some(name) => name,
            None => return self.symbols.clone(),
        };

        match self.group_map.get(group_name) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => self.symbols.clone(),
        }
    }

    /// Refresh quotes from API.
    pub async fn refresh(&mut self) -> Result<()> {
        let symbols = self.active_symbols();
        if symbols.is_empty() {
            return Ok(());
        }

        self.last_refresh_attempt = Some(Instant::now());
        match self.client.get_quotes(&symbols).await {
            Ok(batch) => {
                self.quotes = batch.quotes;
                self.failures.clear(); // Clear old failures before new batch
                self.failures = batch.failures;
                
                // Track fetch time for each quote (for data freshness display)
                let now = Instant::now();
                for quote in &self.quotes {
                    self.quote_fetch_times.insert(quote.symbol.clone(), now);
                }
                
                // Cache fresh quotes
                let quotes_to_cache: Vec<Quote> = self.quotes.clone();
                for quote in quotes_to_cache {
                    self.cache_quote(quote);
                }
                
                self.sort_quotes();
                self.last_refresh = Some(Instant::now());
                self.iteration += 1;
                self.error = None;
                
                // Check price alerts
                self.check_alerts();
                
                if !self.failures.is_empty() {
                    self.set_error(&format!(
                        "Partial data: {} symbol(s) failed",
                        self.failures.len()
                    ));
                }
            }
            Err(e) => {
                let error_msg = format!("API Error: {}", e);
                self.set_error(&error_msg);
            }
        }

        Ok(())
    }

    /// Sort quotes according to current sort settings.
    pub fn sort_quotes(&mut self) {
        let direction = self.sort_direction;

        self.quotes.sort_by(|a, b| {
            let cmp = match self.sort_order {
                SortOrder::Symbol => a.symbol.cmp(&b.symbol),
                SortOrder::Name => a.name.cmp(&b.name),
                SortOrder::Price => a
                    .price
                    .partial_cmp(&b.price)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortOrder::Change => a
                    .change
                    .partial_cmp(&b.change)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortOrder::ChangePercent => a
                    .change_percent
                    .partial_cmp(&b.change_percent)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortOrder::Volume => a.volume.cmp(&b.volume),
                SortOrder::MarketCap => a.market_cap.cmp(&b.market_cap),
            };

            match direction {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });
    }

    /// Toggle sort direction.
    pub fn toggle_sort_direction(&mut self) {
        self.sort_direction = self.sort_direction.toggle();
        self.sort_quotes();
    }

    /// Cycle to next sort order.
    pub fn next_sort_order(&mut self) {
        self.sort_order = self.sort_order.next();
        self.sort_quotes();
    }

    /// Set specific sort order.
    pub fn set_sort_order(&mut self, order: SortOrder) {
        if self.sort_order == order {
            self.toggle_sort_direction();
        } else {
            self.sort_order = order;
            self.sort_direction = SortDirection::Descending;
        }
        self.sort_quotes();
    }

    /// Move selection up.
    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.update_scroll_offset();
        }
    }

    /// Move selection down.
    pub fn select_down(&mut self) {
        if self.selected < self.quotes.len().saturating_sub(1) {
            self.selected += 1;
            self.update_scroll_offset();
        }
    }

    /// Move selection to top.
    pub fn select_top(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Move selection to bottom.
    pub fn select_bottom(&mut self) {
        self.selected = self.quotes.len().saturating_sub(1);
        self.update_scroll_offset();
    }

    /// Update scroll offset to keep selected item visible.
    /// Assumes viewport height of ~20 rows (terminal typical size).
    fn update_scroll_offset(&mut self) {
        let viewport_height = 20; // Approximate typical terminal height minus header/footer
        
        // If selected is above scroll offset, scroll up
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
        
        // If selected is below visible area, scroll down
        if self.selected >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.selected.saturating_sub(viewport_height) + 1;
        }
    }

    /// Set error message with truncation to prevent UI overflow.
    pub fn set_error(&mut self, msg: &str) {
        let truncated = if msg.len() > 100 {
            format!("{}…", &msg[..97])
        } else {
            msg.to_string()
        };
        self.error = Some(truncated);
    }

    /// Set active group and ensure selected index is in bounds.
    pub fn set_active_group(&mut self, index: usize) {
        if index < self.groups.len() {
            self.active_group = index;
            // Bounds check: selected index must be valid for new group
            if self.selected >= self.quotes.len() {
                self.selected = self.quotes.len().saturating_sub(1);
            }
        }
    }

    /// Toggle help display.
    pub fn toggle_help(&mut self) {
        if !self.secure_mode {
            self.show_help = !self.show_help;
        }
    }

    /// Toggle holdings view.
    pub fn toggle_holdings(&mut self) {
        if !self.secure_mode {
            self.show_holdings = !self.show_holdings;
            // Turn off other views when entering holdings
            if self.show_holdings {
                self.show_fundamentals = false;
                self.show_dashboard = false;
            }
        }
    }

    /// Toggle fundamentals display.
    pub fn toggle_fundamentals(&mut self) {
        if !self.secure_mode {
            self.show_fundamentals = !self.show_fundamentals;
            // Turn off other views when entering fundamentals
            if self.show_fundamentals {
                self.show_holdings = false;
                self.show_dashboard = false;
            }
        }
    }

    /// Toggle portfolio dashboard.
    pub fn toggle_dashboard(&mut self) {
        if !self.secure_mode {
            self.show_dashboard = !self.show_dashboard;
            // Turn off other views when entering dashboard
            if self.show_dashboard {
                self.show_holdings = false;
                self.show_fundamentals = false;
                self.show_detail_view = false;
            }
        }
    }

    /// Toggle detail view for selected quote.
    pub fn toggle_detail_view(&mut self) {
        if !self.secure_mode {
            self.show_detail_view = !self.show_detail_view;
            if self.show_detail_view {
                self.show_dashboard = false;
                self.show_holdings = false;
                self.show_fundamentals = false;
            }
        }
    }

    /// Add price to history for sparkline calculation.
    /// Extended buffer from 20 to 100+ prices for accurate technical indicator warmup.
    pub fn update_price_history(&mut self, symbol: &str, price: f64) {
        self.price_history
            .entry(symbol.to_string())
            .or_insert_with(Vec::new)
            .push(price);

        // Keep last 100+ prices for proper EMA warmup and technical indicator accuracy
        // (RSI needs 14+, MACD needs 26+, but 100 gives good signal quality)
        if let Some(history) = self.price_history.get_mut(symbol) {
            if history.len() > 100 {
                history.remove(0);
            }
        }
    }

    /// Calculate RSI (Relative Strength Index) using Wilder's smoothing method.
    /// Standard 14-period calculation.
    /// Wilder's method uses a smoothed moving average, not a simple average.
    pub fn calculate_rsi(&self, symbol: &str) -> Option<f64> {
        let prices = self.price_history.get(symbol)?;
        if prices.len() < 15 {
            return None; // Need at least 15 prices (14 changes)
        }

        // Calculate price changes
        let mut gains = Vec::new();
        let mut losses = Vec::new();
        for i in 1..prices.len() {
            let change = prices[i] - prices[i - 1];
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push(-change);
            }
        }

        // First 14 periods: simple average
        let period = 14;
        let first_avg_gain = gains[0..period].iter().sum::<f64>() / period as f64;
        let first_avg_loss = losses[0..period].iter().sum::<f64>() / period as f64;

        // Wilder's smoothing for subsequent periods
        let mut avg_gain = first_avg_gain;
        let mut avg_loss = first_avg_loss;

        for i in period..gains.len() {
            avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;
        }

        if avg_loss == 0.0 {
            return Some(100.0); // All gains, no losses
        }

        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }

    /// Calculate simple moving average.
    pub fn calculate_sma(&self, symbol: &str, period: usize) -> Option<f64> {
        let prices = self.price_history.get(symbol)?;
        if prices.len() < period {
            return None;
        }

        let sum: f64 = prices.iter().rev().take(period).sum();
        Some(sum / period as f64)
    }

    /// Calculate MACD (signal, macd_line, histogram).
    /// MACD = EMA(12) - EMA(26)
    /// Signal = EMA(9) of MACD line
    /// Histogram = MACD - Signal
    pub fn calculate_macd(&self, symbol: &str) -> Option<(f64, f64, f64)> {
        let prices = self.price_history.get(symbol)?;
        if prices.len() < 26 {
            return None; // Need at least 26 prices for EMA(26)
        }

        let ema12 = self.calculate_ema(prices, 12)?;
        let ema26 = self.calculate_ema(prices, 26)?;

        let macd_line = ema12 - ema26;
        
        // Signal is 9-period EMA of MACD line history (proper implementation)
        // For simplicity with limited history, we approximate by taking recent MACD values
        // In production, you'd store MACD history and compute EMA(9) of it
        // For now, use EMA-style smoothing of the current MACD
        let signal = self.calculate_macd_signal(&macd_line, prices, 9)?;
        let histogram = macd_line - signal;

        Some((signal, macd_line, histogram))
    }

    /// Helper to calculate MACD signal line approximation.
    /// With limited price history, we approximate by using EMA smoothing factor.
    fn calculate_macd_signal(&self, _macd_line: &f64, prices: &[f64], signal_period: usize) -> Option<f64> {
        if prices.len() < 26 {
            return None;
        }
        
        // Calculate recent MACD values for smoothing
        // Use last few prices to build a small MACD series
        let mut macd_values = Vec::new();
        let lookback = 5; // Use last 5 MACD values for signal smoothing
        
        for i in (prices.len() - lookback)..prices.len() {
            if let (Some(ema12), Some(ema26)) = (
                self.calculate_ema(&prices[0..=i], 12),
                self.calculate_ema(&prices[0..=i], 26),
            ) {
                macd_values.push(ema12 - ema26);
            }
        }
        
        if macd_values.is_empty() {
            return None;
        }
        
        // Simple EMA of MACD values
        let multiplier = 2.0 / (signal_period as f64 + 1.0);
        let mut ema = macd_values[0];
        for macd in &macd_values[1..] {
            ema = (macd - ema) * multiplier + ema;
        }
        
        Some(ema)
    }

    fn calculate_ema(&self, prices: &[f64], period: usize) -> Option<f64> {
        if prices.len() < period {
            return None;
        }

        let multiplier = 2.0 / (period as f64 + 1.0);
        let start_idx = prices.len() - period;
        let mut ema = prices[start_idx..].iter().sum::<f64>() / period as f64;

        // Process remaining prices in chronological order
        for price in &prices[start_idx + 1..] {
            ema = (price - ema) * multiplier + ema;
        }

        Some(ema)
    }

    /// Get sparkline ASCII for a symbol (shows price trend).
    pub fn get_sparkline(&self, symbol: &str) -> String {
        const SPARK_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

        if let Some(prices) = self.price_history.get(symbol) {
            if prices.len() < 2 {
                return String::new();
            }

            // Take the last 5 prices (most recent)
            let recent: Vec<f64> = prices.iter().rev().take(5).copied().collect();
            let min = recent.iter().copied().fold(f64::INFINITY, f64::min);
            let max = recent.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let range = max - min;

            if range == 0.0 {
                return "▄".repeat(5);
            }

            // Reverse back to chronological order (oldest to newest)
            recent
                .iter()
                .rev()
                .map(|&p| {
                    let normalized = (p - min) / range;
                    let index = ((normalized * 7.99) as usize).min(7);
                    SPARK_CHARS[index]
                })
                .collect()
        } else {
            String::new()
        }
    }

    /// Get data age for a symbol in seconds.
    pub fn get_data_age(&self, symbol: &str) -> u64 {
        self.quote_fetch_times
            .get(symbol)
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(999)
    }

    /// Quit the application.
    pub fn quit(&mut self) {
        self.running = false;
    }

    /// Check if max iterations reached.
    pub fn should_quit(&self) -> bool {
        !self.running || (self.max_iterations > 0 && self.iteration >= self.max_iterations)
    }

    /// Start search mode with a query
    pub fn start_search(&mut self, query: String) {
        self.search_query = Some(query.to_lowercase());
        self.update_filtered_quotes();
        self.selected = 0;
    }

    /// Clear search mode
    pub fn clear_search(&mut self) {
        self.search_query = None;
        self.filtered_quotes.clear();
        self.selected = 0;
    }

    /// Enter search input mode
    pub fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_input.clear();
    }

    /// Exit search input mode and apply search
    pub fn exit_search_mode(&mut self) {
        self.search_mode = false;
        if !self.search_input.is_empty() {
            self.start_search(self.search_input.clone());
        } else {
            self.clear_search();
        }
    }

    /// Add character to search input
    pub fn search_input_push(&mut self, c: char) {
        self.search_input.push(c);
        self.start_search(self.search_input.clone());
    }

    /// Remove last character from search input
    pub fn search_input_pop(&mut self) {
        self.search_input.pop();
        if self.search_input.is_empty() {
            self.clear_search();
        } else {
            self.start_search(self.search_input.clone());
        }
    }

    /// Update filtered quotes based on search query
    pub fn update_filtered_quotes(&mut self) {
        if let Some(ref query) = self.search_query {
            self.filtered_quotes = self
                .quotes
                .iter()
                .filter(|q| {
                    q.symbol.to_lowercase().contains(query)
                        || q.name.to_lowercase().contains(query)
                })
                .cloned()
                .collect();
        }
    }

    /// Get current display quotes (filtered or all)
    pub fn display_quotes(&self) -> &[Quote] {
        if self.search_query.is_some() {
            &self.filtered_quotes
        } else {
            &self.quotes
        }
    }

    /// Add a price alert
    pub fn add_alert(&mut self, symbol: &str, condition: AlertCondition, price: f64) {
        self.alerts
            .entry(symbol.to_string())
            .or_insert_with(Vec::new)
            .push((condition, price));
    }

    /// Remove an alert
    pub fn remove_alert(&mut self, symbol: &str, index: usize) {
        if let Some(alerts) = self.alerts.get_mut(symbol) {
            if index < alerts.len() {
                alerts.remove(index);
            }
        }
    }

    /// Check alerts and populate triggered_alerts
    pub fn check_alerts(&mut self) {
        self.triggered_alerts.clear();
        for quote in &self.quotes {
            if let Some(alerts) = self.alerts.get(&quote.symbol) {
                for &(condition, target_price) in alerts {
                    let triggered = match condition {
                        AlertCondition::Above => quote.price >= target_price,
                        AlertCondition::Below => quote.price <= target_price,
                        AlertCondition::Equal => (quote.price - target_price).abs() < 0.01,
                    };
                    if triggered {
                        self.triggered_alerts
                            .push((quote.symbol.clone(), condition, target_price, quote.price));
                        
                        // Play audible alert if enabled
                        if self.audio_alerts {
                            crate::audio::play_sound_async(crate::audio::AlertSound::Double);
                        }
                    }
                }
            }
        }
    }

    /// Start alert setup for a symbol
    pub fn start_alert_setup(&mut self, symbol: String) {
        self.alert_setup_mode = Some((symbol, AlertSetupMode::SelectCondition(0)));
    }

    /// Move to next condition (Above -> Below -> Equal -> Above)
    pub fn alert_next_condition(&mut self) {
        if let Some((symbol, AlertSetupMode::SelectCondition(idx))) = self.alert_setup_mode.clone() {
            let next = (idx + 1) % 3;
            self.alert_setup_mode = Some((symbol, AlertSetupMode::SelectCondition(next)));
        }
    }

    /// Move to previous condition
    pub fn alert_prev_condition(&mut self) {
        if let Some((symbol, AlertSetupMode::SelectCondition(idx))) = self.alert_setup_mode.clone() {
            let prev = if idx == 0 { 2 } else { idx - 1 };
            self.alert_setup_mode = Some((symbol, AlertSetupMode::SelectCondition(prev)));
        }
    }

    /// Move to price entry
    pub fn alert_enter_price(&mut self) {
        if let Some((symbol, AlertSetupMode::SelectCondition(idx))) = self.alert_setup_mode.clone() {
            let conditions = [AlertCondition::Above, AlertCondition::Below, AlertCondition::Equal];
            let selected = conditions[idx];
            self.alert_setup_mode = Some((symbol, AlertSetupMode::EnterPrice(selected, String::new())));
        }
    }

    /// Add character to price input
    pub fn alert_price_push(&mut self, c: char) {
        if let Some((symbol, AlertSetupMode::EnterPrice(condition, mut price))) = self.alert_setup_mode.clone() {
            if c.is_numeric() || (c == '.' && !price.contains('.')) {
                price.push(c);
                self.alert_setup_mode = Some((symbol, AlertSetupMode::EnterPrice(condition, price)));
            }
        }
    }

    /// Remove last character from price input
    pub fn alert_price_pop(&mut self) {
        if let Some((symbol, AlertSetupMode::EnterPrice(condition, mut price))) = self.alert_setup_mode.clone() {
            price.pop();
            self.alert_setup_mode = Some((symbol, AlertSetupMode::EnterPrice(condition, price)));
        }
    }

    /// Finalize alert setup
    pub fn alert_confirm(&mut self) -> bool {
        if let Some((symbol, AlertSetupMode::EnterPrice(condition, price_str))) = self.alert_setup_mode.clone() {
            if let Ok(price) = price_str.parse::<f64>() {
                self.add_alert(&symbol, condition, price);
                self.alert_setup_mode = None;
                return true;
            }
        }
        false
    }

    /// Cancel alert setup
    pub fn alert_cancel(&mut self) {
        self.alert_setup_mode = None;
    }

    /// Get quotes from cache if available and fresh, else return None
    pub fn get_cached_quote(&self, symbol: &str) -> Option<&Quote> {
        self.quote_cache.get(symbol).and_then(|(quote, fetch_time)| {
            if fetch_time.elapsed() < self.cache_duration {
                Some(quote)
            } else {
                None
            }
        })
    }

    /// Cache a quote
    pub fn cache_quote(&mut self, quote: Quote) {
        self.quote_cache
            .insert(quote.symbol.clone(), (quote, Instant::now()));
    }

    /// Clear expired cache entries
    pub fn clean_cache(&mut self) {
        let now = Instant::now();
        self.quote_cache.retain(|_, (_, fetch_time)| {
            now.duration_since(*fetch_time) < self.cache_duration
        });
    }

    /// Save alerts to configuration for persistence between sessions.
    pub fn save_alerts_to_config(&self, config: &mut crate::config::Config) {
        config.alerts.clear();

        for (symbol, alerts) in &self.alerts {
            let alert_configs: Vec<crate::config::AlertConfig> = alerts
                .iter()
                .map(|(condition, price)| {
                    let condition_str = match condition {
                        AlertCondition::Above => "above",
                        AlertCondition::Below => "below",
                        AlertCondition::Equal => "equal",
                    };
                    crate::config::AlertConfig {
                        condition: condition_str.to_string(),
                        price: *price,
                    }
                })
                .collect();

            if !alert_configs.is_empty() {
                config.alerts.insert(symbol.clone(), alert_configs);
            }
        }
    }

    /// Get total portfolio value.
    pub fn total_portfolio_value(&self) -> f64 {
        self.quotes
            .iter()
            .filter_map(|q| {
                self.holdings
                    .get(&q.symbol)
                    .map(|h| h.current_value(q.price))
            })
            .sum()
    }

    /// Get total portfolio cost.
    pub fn total_portfolio_cost(&self) -> f64 {
        self.holdings.values().map(|h| h.total_cost()).sum()
    }

    /// Get total portfolio profit/loss.
    pub fn total_portfolio_pnl(&self) -> f64 {
        self.total_portfolio_value() - self.total_portfolio_cost()
    }

    /// Get today's portfolio change.
    pub fn today_portfolio_change(&self) -> f64 {
        self.quotes
            .iter()
            .filter_map(|q| self.holdings.get(&q.symbol).map(|h| h.quantity * q.change))
            .sum()
    }

    /// Add a symbol to watch.
    /// For when FOMO hits and you need to track one more meme stock.
    #[allow(dead_code)] // Interactive symbol adding - coming in v2.0 (probably)
    pub fn add_symbol(&mut self, symbol: &str) {
        let expanded = expand_symbol(symbol);
        if !self.symbols.contains(&expanded) {
            self.symbols.push(expanded);
        }
    }

    /// Remove a symbol from watch.
    /// Denial is the first stage of grief. Removing it from your watchlist is the second.
    #[allow(dead_code)] // Interactive symbol removal - for those who can't handle the truth
    pub fn remove_symbol(&mut self, symbol: &str) {
        let expanded = expand_symbol(symbol);
        self.symbols.retain(|s| s != &expanded);
        self.quotes.retain(|q| q.symbol != expanded);
        if self.selected >= self.quotes.len() {
            self.selected = self.quotes.len().saturating_sub(1);
        }
    }

    /// Get the currently selected quote.
    /// Returns the quote you're currently staring at in disbelief.
    #[allow(dead_code)] // Used by future detail view feature
    pub fn selected_quote(&self) -> Option<&Quote> {
        self.quotes.get(self.selected)
    }

    /// Get time since last refresh as human readable string.
    pub fn time_since_refresh(&self) -> String {
        match self.last_refresh {
            Some(t) => {
                let elapsed = t.elapsed().as_secs();
                if elapsed < 60 {
                    format!("{}s ago", elapsed)
                } else {
                    format!("{}m ago", elapsed / 60)
                }
            }
            None => "never".to_string(),
        }
    }
}

#[cfg(test)]
fn create_test_quote(symbol: &str, price: f64, change_percent: f64) -> Quote {
    Quote {
        symbol: symbol.to_string(),
        name: format!("{} Corp", symbol),
        price,
        change: price * change_percent / 100.0,
        change_percent,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_by_change_percent_descending() {
        let mut app = App {
            quotes: vec![
                create_test_quote("A", 100.0, -1.0),
                create_test_quote("B", 100.0, 2.5),
                create_test_quote("C", 100.0, 0.0),
            ],
            sort_order: SortOrder::ChangePercent,
            sort_direction: SortDirection::Descending,
            ..Default::default()
        };

        app.sort_quotes();
        
        assert_eq!(app.quotes[0].symbol, "B"); // 2.5%
        assert_eq!(app.quotes[1].symbol, "C"); // 0.0%
        assert_eq!(app.quotes[2].symbol, "A"); // -1.0%
    }

    #[test]
    fn test_sort_by_change_percent_ascending() {
        let mut app = App {
            quotes: vec![
                create_test_quote("A", 100.0, -1.0),
                create_test_quote("B", 100.0, 2.5),
                create_test_quote("C", 100.0, 0.0),
            ],
            sort_order: SortOrder::ChangePercent,
            sort_direction: SortDirection::Ascending,
            ..Default::default()
        };

        app.sort_quotes();
        
        assert_eq!(app.quotes[0].symbol, "A"); // -1.0%
        assert_eq!(app.quotes[1].symbol, "C"); // 0.0%
        assert_eq!(app.quotes[2].symbol, "B"); // 2.5%
    }

    #[test]
    fn test_sort_by_symbol() {
        let mut app = App {
            quotes: vec![
                create_test_quote("ZZPQ", 100.0, 0.0),
                create_test_quote("AAPL", 100.0, 0.0),
                create_test_quote("MSFT", 100.0, 0.0),
            ],
            sort_order: SortOrder::Symbol,
            sort_direction: SortDirection::Ascending,
            ..Default::default()
        };

        app.sort_quotes();
        
        assert_eq!(app.quotes[0].symbol, "AAPL");
        assert_eq!(app.quotes[1].symbol, "MSFT");
        assert_eq!(app.quotes[2].symbol, "ZZPQ");
    }

    #[test]
    fn test_sort_by_price() {
        let mut app = App {
            quotes: vec![
                create_test_quote("A", 50.0, 0.0),
                create_test_quote("B", 200.0, 0.0),
                create_test_quote("C", 100.0, 0.0),
            ],
            sort_order: SortOrder::Price,
            sort_direction: SortDirection::Descending,
            ..Default::default()
        };

        app.sort_quotes();
        
        assert_eq!(app.quotes[0].price, 200.0);
        assert_eq!(app.quotes[1].price, 100.0);
        assert_eq!(app.quotes[2].price, 50.0);
    }

    #[test]
    fn test_set_error_truncation() {
        let mut app = App::default();
        let long_error = "x".repeat(150);
        app.set_error(&long_error);
        
        assert!(app.error.is_some());
        let error = app.error.unwrap();
        assert_eq!(error.len(), 100); // 97 chars + 1 for …
        assert!(error.ends_with("…"));
    }

    #[test]
    fn test_set_error_short_message() {
        let mut app = App::default();
        app.set_error("Short error");
        
        assert_eq!(app.error, Some("Short error".to_string()));
    }

    #[test]
    fn test_set_active_group_bounds_check() {
        let mut app = App {
            groups: vec!["Group1".to_string(), "Group2".to_string()],
            quotes: vec![create_test_quote("A", 100.0, 0.0)],
            selected: 0,
            ..Default::default()
        };

        app.set_active_group(0);
        assert_eq!(app.active_group, 0);

        app.set_active_group(1);
        assert_eq!(app.active_group, 1);

        // Out of bounds should not change
        app.set_active_group(5);
        assert_eq!(app.active_group, 1);
    }

    #[test]
    fn test_select_navigation() {
        let mut app = App {
            quotes: vec![
                create_test_quote("A", 100.0, 0.0),
                create_test_quote("B", 100.0, 0.0),
                create_test_quote("C", 100.0, 0.0),
            ],
            selected: 0,
            ..Default::default()
        };

        app.select_down();
        assert_eq!(app.selected, 1);

        app.select_up();
        assert_eq!(app.selected, 0);

        app.select_bottom();
        assert_eq!(app.selected, 2);

        app.select_top();
        assert_eq!(app.selected, 0);
    }
}
