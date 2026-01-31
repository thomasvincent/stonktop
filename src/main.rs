//! Stonktop - A top-like terminal UI for stock and crypto prices.
//!
//! "Stonks only go up" - Famous last words
//!
//! A terminal-based stock and cryptocurrency price monitor that brings
//! the thrill of watching your portfolio fluctuate directly to your
//! command line. Now you can lose money AND look like a hacker!

mod api;
mod app;
mod audio;
mod cli;
mod config;
mod export;
mod models;
mod ui;

use anyhow::Result;
use app::App;
use cli::Args;
use config::Config;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

/// Guard struct to ensure terminal state is cleaned up even on panic.
/// Implements Drop to restore terminal to normal mode.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Always attempt to restore terminal state, even if already disabled
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse_args();

    // Load configuration
    let config = if let Some(ref path) = args.config {
        match Config::load(path) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Error: Failed to load config file: {}", e);
                eprintln!("Please check that the file exists and is valid TOML.");
                std::process::exit(1);
            }
        }
    } else {
        Config::load_or_default()
    };

    // Create application state
    let mut app = App::new(&args, &config)?;

    // Check if we have any symbols to watch
    if app.symbols.is_empty() {
        eprintln!("Error: No symbols to watch.");
        eprintln!("Provide symbols via -s flag or config file.");
        eprintln!();
        eprintln!("Example: stonktop -s AAPL,GOOGL,BTC-USD");
        eprintln!();
        eprintln!(
            "Or create a config file at {:?}",
            Config::default_config_path()
        );
        eprintln!();
        eprintln!("Sample config:");
        eprintln!("{}", config::sample_config());
        std::process::exit(1);
    }

    // Handle export format if specified
    if let Some(export_format) = args.export {
        return run_export(&mut app, export_format).await;
    }

    // Run in batch mode or interactive mode
    if app.batch_mode {
        run_batch(&mut app).await
    } else {
        run_interactive(&mut app).await
    }
}

/// Run with data export format.
async fn run_export(app: &mut App, format: cli::ExportFormat) -> Result<()> {
    // Fetch quotes once
    app.refresh().await?;
    
    // Convert format to export module format
    let export_format = match format {
        cli::ExportFormat::Text => export::ExportFormat::Text,
        cli::ExportFormat::Csv => export::ExportFormat::Csv,
        cli::ExportFormat::Json => export::ExportFormat::Json,
    };
    
    // Export and print
    let output = export::export_quotes(&app.display_quotes(), export_format);
    println!("{}", output);
    
    Ok(())
}

/// Run in batch mode (non-interactive, like top -b).
async fn run_batch(app: &mut App) -> Result<()> {
    loop {
        app.refresh().await?;
        ui::render_batch(app);

        if app.should_quit() {
            break;
        }

        // Add small random jitter to prevent API hammering at exact intervals
        let jitter_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() % 100) as u64;
        tokio::time::sleep(app.refresh_interval + Duration::from_millis(jitter_ms)).await;
    }

    Ok(())
}

/// Run in interactive mode with TUI.
async fn run_interactive(app: &mut App) -> Result<()> {
    // Create guard that will restore terminal on drop (panic or normal return)
    let _guard = TerminalGuard;
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initial fetch
    app.refresh().await?;

    // Main loop
    let result = run_app(&mut terminal, app).await;

    // Explicit cleanup (guard will also run on drop)
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main application loop.
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_key_time = std::time::Instant::now();

    loop {
        // Draw UI
        terminal.draw(|f| ui::render(f, app))?;

        // Handle events with timeout
        if crossterm::event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                // Debounce rapid keypresses (ignore if within 50ms of last key)
                if last_key_time.elapsed() > Duration::from_millis(50) {
                    last_key_time = std::time::Instant::now();
                    
                    // If in search mode, handle search input specially
                    if app.search_mode {
                        match key.code {
                            KeyCode::Char(c) => app.search_input_push(c),
                            KeyCode::Backspace => app.search_input_pop(),
                            KeyCode::Enter | KeyCode::Esc => app.exit_search_mode(),
                            _ => {}
                        }
                    } else if app.alert_setup_mode.is_some() {
                        // Handle alert setup mode
                        match key.code {
                            KeyCode::Left => app.alert_prev_condition(),
                            KeyCode::Right => app.alert_next_condition(),
                            KeyCode::Down => {
                                if let Some((_, crate::app::AlertSetupMode::SelectCondition(_))) = &app.alert_setup_mode {
                                    app.alert_enter_price();
                                }
                            }
                            KeyCode::Char(c) => {
                                if let Some((_, crate::app::AlertSetupMode::EnterPrice(..))) = &app.alert_setup_mode {
                                    app.alert_price_push(c);
                                }
                            }
                            KeyCode::Backspace => app.alert_price_pop(),
                            KeyCode::Enter => {
                                app.alert_confirm();
                            }
                            KeyCode::Esc => app.alert_cancel(),
                            _ => {}
                        }
                    } else if app.secure_mode {
                        // Skip if secure mode and it's a modifying command
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => app.quit(),
                            KeyCode::Up | KeyCode::Char('k') => app.select_up(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_down(),
                            _ => {}
                        }
                    } else {
                        handle_key_event(app, key.code, key.modifiers);
                    }
                }
            }
        }

        // Check if we should quit
        if app.should_quit() {
            break;
        }

        // Refresh data if needed, with small random jitter to avoid API hammering
        if should_refresh_with_jitter(app) {
            app.refresh().await?;
        }
    }

    Ok(())
}

/// Determine if refresh is needed with small random jitter.
/// Jitter is ±10% of refresh interval to desynchronize API calls from precise interval timing.
fn should_refresh_with_jitter(app: &App) -> bool {
    match app.last_refresh {
        None => true,
        Some(last) => {
            let elapsed = last.elapsed();
            let refresh_interval = app.refresh_interval;
            
            // Add jitter: ±10% of refresh interval
            let jitter_ms: u64 = ((std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as u64) / 1_000_000) % ((refresh_interval.as_millis() as u64 / 10) + 1);
            
            let jitter = Duration::from_millis(jitter_ms);
            let base_ms = refresh_interval.as_millis() as u64;
            let min_ms = (base_ms * 9) / 10;  // 90% of interval
            let effective_interval = Duration::from_millis(min_ms) + jitter;
            
            elapsed >= effective_interval
        }
    }
}

/// Open a URL in the default browser.
fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("open").arg(url).output()?;
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        Command::new("xdg-open").arg(url).output()?;
    }
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("cmd")
            .args(&["/C", "start", url])
            .output()?;
    }
    Ok(())
}

/// Handle keyboard input.
fn handle_key_event(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    // Close help overlay on any key
    if app.show_help {
        app.show_help = false;
        return;
    }

    // Clear error on any key
    if app.error.is_some() {
        app.error = None;
        return;
    }

    match code {
        // Quit
        KeyCode::Char('q') | KeyCode::Esc => app.quit(),
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => app.quit(),

        // Navigation
        KeyCode::Up | KeyCode::Char('k') => app.select_up(),
        KeyCode::Down | KeyCode::Char('j') => app.select_down(),
        KeyCode::Home | KeyCode::Char('g') => app.select_top(),
        KeyCode::End | KeyCode::Char('G') => app.select_bottom(),
        KeyCode::Left => {
            // Previous group
            if !app.groups.is_empty() {
                let prev_group = if app.active_group == 0 {
                    app.groups.len() - 1
                } else {
                    app.active_group - 1
                };
                app.set_active_group(prev_group);
            }
        }
        KeyCode::Right => {
            // Next group
            if !app.groups.is_empty() {
                let next_group = (app.active_group + 1) % app.groups.len();
                app.set_active_group(next_group);
            }
        }
        KeyCode::PageUp => {
            for _ in 0..10 {
                app.select_up();
            }
        }
        KeyCode::PageDown => {
            for _ in 0..10 {
                app.select_down();
            }
        }

        // Sorting
        KeyCode::Char('s') => app.next_sort_order(),
        KeyCode::Char('r') => app.toggle_sort_direction(),
        KeyCode::Char('1') => app.set_sort_order(models::SortOrder::Symbol),
        KeyCode::Char('2') => app.set_sort_order(models::SortOrder::Name),
        KeyCode::Char('3') => app.set_sort_order(models::SortOrder::Price),
        KeyCode::Char('4') => app.set_sort_order(models::SortOrder::Change),
        KeyCode::Char('5') => app.set_sort_order(models::SortOrder::ChangePercent),
        KeyCode::Char('6') => app.set_sort_order(models::SortOrder::Volume),
        KeyCode::Char('7') => app.set_sort_order(models::SortOrder::MarketCap),

        // Display toggles
        KeyCode::Char('h') => app.toggle_holdings(),
        KeyCode::Char('f') => app.toggle_fundamentals(),
        KeyCode::Char('d') => app.toggle_dashboard(),
        KeyCode::Char('?') => app.toggle_help(),

        // Search
        KeyCode::Char('/') => app.enter_search_mode(),

        // Alerts
        KeyCode::Char('a') => {
            let quotes = app.display_quotes();
            if !quotes.is_empty() && app.selected < quotes.len() {
                app.start_alert_setup(quotes[app.selected].symbol.clone());
            }
        }

        // Detail view
        KeyCode::Enter => app.toggle_detail_view(),

        // News (open in browser when in detail view)
        KeyCode::Char('n') => {
            if app.show_detail_view {
                let quotes = app.display_quotes();
                if !quotes.is_empty() && app.selected < quotes.len() {
                    let symbol = &quotes[app.selected].symbol;
                    // Build news URL
                    let url = format!("https://finance.yahoo.com/quote/{}/news", symbol);
                    let _ = open_browser(&url);
                }
            }
        }

        // Refresh
        KeyCode::Char(' ') | KeyCode::Char('R') => {
            app.last_refresh = None; // Force refresh on next tick
        }

        // Groups - cycle through watchlists with bounds checking
        KeyCode::Tab => {
            if !app.groups.is_empty() {
                let next_group = (app.active_group + 1) % app.groups.len();
                app.set_active_group(next_group);
            }
        }

        _ => {}
    }
}
