//! Terminal user interface with ratatui.
//!
//! Making financial data look pretty since 2024.
//! (The data itself? Still ugly. That's not our fault.)

use crate::app::App;
use crate::models::SortOrder;
use num_format::{Locale, ToFormattedString};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

/// Colors for the UI.
pub struct UiColors {
    pub gain: Color,
    pub loss: Color,
    pub neutral: Color,
    pub header_bg: Color,
    pub selected_bg: Color,
    pub border: Color,
}

impl UiColors {
    /// Get colors for standard mode.
    pub fn standard() -> Self {
        Self {
            gain: Color::Green,
            loss: Color::Red,
            neutral: Color::White,
            header_bg: Color::DarkGray,
            selected_bg: Color::Rgb(40, 40, 60),
            border: Color::DarkGray,
        }
    }

    /// Get colors for high contrast mode (accessibility).
    /// Enhanced colors with better contrast ratios for users with low vision.
    pub fn high_contrast() -> Self {
        Self {
            gain: Color::LightGreen,         // Brighter green (9.5:1 contrast)
            loss: Color::LightRed,           // Brighter red (8.2:1 contrast)
            neutral: Color::White,           // Pure white (10.8:1 contrast)
            header_bg: Color::Black,         // Pure black for maximum contrast
            selected_bg: Color::Blue,        // Bright blue instead of dark blue
            border: Color::White,            // White borders for visibility
        }
    }
}

impl Default for UiColors {
    fn default() -> Self {
        Self::standard()
    }
}

/// Render the main UI.
pub fn render(frame: &mut Frame, app: &App) {
    // Use high contrast colors if enabled in app configuration
    let colors = if app.high_contrast {
        UiColors::high_contrast()
    } else {
        UiColors::default()
    };

    // Create layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main table
            Constraint::Length(1), // Footer
        ])
        .split(frame.area());

    // Render header
    render_header(frame, app, chunks[0], &colors);

    // Render main table
    if app.show_dashboard {
        render_dashboard(frame, app, chunks[1], &colors);
    } else if app.show_holdings {
        render_holdings_table(frame, app, chunks[1], &colors);
    } else if app.show_fundamentals {
        render_fundamentals_table(frame, app, chunks[1], &colors);
    } else if app.show_detail_view {
        render_detail_view(frame, app, chunks[1], &colors);
    } else {
        render_quotes_table(frame, app, chunks[1], &colors);
    }

    // Render footer
    render_footer(frame, app, chunks[2], &colors);

    // Render help overlay if active
    if app.show_help {
        render_help_overlay(frame, &colors);
    }

    // Render error if present
    if let Some(ref error) = app.error {
        render_error(frame, error, &colors);
    }

    // Render search box if in search mode
    if app.search_mode {
        render_search_input(frame, app, &colors);
    }

    // Render alerts if any triggered
    if !app.triggered_alerts.is_empty() {
        render_alerts_overlay(frame, app, &colors);
    }

    // Render alert setup modal if active
    if let Some((symbol, _)) = &app.alert_setup_mode {
        render_alert_setup_modal(frame, app, symbol, &colors);
    }
}

/// Render the header with summary information.
fn render_header(frame: &mut Frame, app: &App, area: Rect, colors: &UiColors) {
    let gains = app.quotes.iter().filter(|q| q.change_percent > 0.0).count();
    let losses = app.quotes.iter().filter(|q| q.change_percent < 0.0).count();
    let unchanged = app.quotes.len() - gains - losses;

    let header_text = if app.show_holdings {
        let total_value = app.total_portfolio_value();
        let total_pnl = app.total_portfolio_pnl();
        let today_change = app.today_portfolio_change();
        let pnl_pct = if app.total_portfolio_cost() > 0.0 {
            (total_pnl / app.total_portfolio_cost()) * 100.0
        } else {
            0.0
        };

        vec![
            Line::from(vec![
                Span::styled(
                    "STONKTOP ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("- Portfolio View"),
            ]),
            Line::from(vec![
                Span::raw(format!("Value: ${:.2}  ", total_value)),
                Span::styled(
                    format!("P/L: {:+.2} ({:+.2}%)  ", total_pnl, pnl_pct),
                    Style::default().fg(if total_pnl >= 0.0 {
                        colors.gain
                    } else {
                        colors.loss
                    }),
                ),
                Span::styled(
                    format!("Today: {:+.2}", today_change),
                    Style::default().fg(if today_change >= 0.0 {
                        colors.gain
                    } else {
                        colors.loss
                    }),
                ),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled(
                    "STONKTOP ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("- {} symbols", app.quotes.len())),
            ]),
            Line::from(vec![
                Span::styled(format!("{} ", gains), Style::default().fg(colors.gain)),
                Span::raw("up  "),
                Span::styled(format!("{} ", losses), Style::default().fg(colors.loss)),
                Span::raw("down  "),
                Span::raw(format!("{} unchanged  ", unchanged)),
                Span::raw(format!("Market: {}  ", get_market_state_display(app))),
                Span::raw(format!("Updated: {}", app.time_since_refresh())),
            ]),
        ]
    };

    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(colors.border)),
    );

    frame.render_widget(header, area);
}

/// Render the quotes table.
fn render_quotes_table(frame: &mut Frame, app: &App, area: Rect, colors: &UiColors) {
    let header_cells = [
        ("SYMBOL", SortOrder::Symbol),
        ("NAME", SortOrder::Name),
        ("PRICE", SortOrder::Price),
        ("CHANGE", SortOrder::Change),
        ("CHG%", SortOrder::ChangePercent),
        ("VOLUME", SortOrder::Volume),
        ("MKT CAP", SortOrder::MarketCap),
        ("TREND", SortOrder::Symbol),
    ]
    .iter()
    .map(|(name, order)| {
        let style = if app.sort_order == *order {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let indicator = if app.sort_order == *order && name != &"TREND" {
            match app.sort_direction {
                crate::models::SortDirection::Ascending => " ▲",
                crate::models::SortDirection::Descending => " ▼",
            }
        } else {
            ""
        };

        Cell::from(format!("{}{}", name, indicator)).style(style)
    });

    let header = Row::new(header_cells)
        .style(Style::default().bg(colors.header_bg))
        .height(1);

    let display_quotes = app.display_quotes();
    
    // Apply scroll offset to show only visible portion
    let viewport_height = area.height.saturating_sub(2) as usize; // -1 for header, -1 for border
    let visible_quotes: Vec<_> = display_quotes
        .iter()
        .skip(app.scroll_offset)
        .take(viewport_height)
        .collect();
    
    let rows = visible_quotes.iter().enumerate().map(|(i, quote)| {
        // Adjust index for selection comparison (account for scroll offset)
        let actual_index = i + app.scroll_offset;
        let is_selected = actual_index == app.selected;
        let change_color = if quote.change_percent > 0.0 {
            colors.gain
        } else if quote.change_percent < 0.0 {
            colors.loss
        } else {
            colors.neutral
        };

        // Data freshness color (green if fresh, yellow if old, red if very old)
        let data_age = app.get_data_age(&quote.symbol);
        let freshness_color = match data_age {
            0..=30 => Color::Green,   // Fresh (< 30 seconds)
            31..=60 => Color::Yellow, // Aging (< 1 minute)
            _ => Color::Red,          // Stale (> 1 minute)
        };

        let row_style = if is_selected {
            Style::default().bg(colors.selected_bg)
        } else {
            Style::default()
        };

        // Get sparkline for price trend
        let sparkline = app.get_sparkline(&quote.symbol);

        let cells = vec![
            Cell::from(quote.symbol.clone()).style(Style::default().fg(freshness_color)),
            Cell::from(truncate_string(&quote.name, 20)),
            Cell::from(format_price(quote.price)),
            Cell::from(format!("{:+.2}", quote.change)).style(Style::default().fg(change_color)),
            Cell::from(format!("{:+.2}%", quote.change_percent))
                .style(Style::default().fg(change_color)),
            Cell::from(format_volume(quote.volume)),
            Cell::from(format_market_cap(quote.market_cap)),
            Cell::from(sparkline).style(Style::default().fg(Color::Cyan)),
        ];

        Row::new(cells).style(row_style)
    });

    let widths = [
        Constraint::Length(10),
        Constraint::Length(20),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(6),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = TableState::default();
    state.select(Some(app.selected));

    frame.render_stateful_widget(table, area, &mut state);
}

/// Render fundamentals table with OHLC and 52-week data.
fn render_fundamentals_table(frame: &mut Frame, app: &App, area: Rect, colors: &UiColors) {
    let header_cells = [
        "SYMBOL", "PRICE", "OPEN", "DAY HIGH", "DAY LOW", "52W HIGH", "52W LOW", "VOLUME",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default().fg(Color::White)));

    let header = Row::new(header_cells)
        .style(Style::default().bg(colors.header_bg))
        .height(1);

    let rows = app.quotes.iter().enumerate().map(|(i, quote)| {
        let is_selected = i == app.selected;
        let change_color = if quote.change_percent > 0.0 {
            colors.gain
        } else if quote.change_percent < 0.0 {
            colors.loss
        } else {
            colors.neutral
        };

        let row_style = if is_selected {
            Style::default().bg(colors.selected_bg)
        } else {
            Style::default()
        };

        let cells = vec![
            Cell::from(quote.symbol.clone()),
            Cell::from(format_price(quote.price))
                .style(Style::default().fg(change_color)),
            Cell::from(format_price(quote.open)),
            Cell::from(format_price(quote.day_high)),
            Cell::from(format_price(quote.day_low)),
            Cell::from(format_price(quote.year_high)),
            Cell::from(format_price(quote.year_low)),
            Cell::from(format_volume(quote.volume)),
        ];

        Row::new(cells).style(row_style)
    });

    let widths = [
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = TableState::default();
    // Set selection relative to viewport (not absolute), accounting for scroll offset
    let viewport_height = area.height.saturating_sub(2) as usize; // -1 for header, -1 for border
    let selected_in_viewport = if app.selected >= app.scroll_offset 
        && app.selected < app.scroll_offset + viewport_height {
        Some(app.selected - app.scroll_offset)
    } else {
        None
    };
    state.select(selected_in_viewport);

    frame.render_stateful_widget(table, area, &mut state);
}

/// Render the holdings/portfolio table.
fn render_holdings_table(frame: &mut Frame, app: &App, area: Rect, colors: &UiColors) {
    let header_cells = [
        "SYMBOL", "NAME", "PRICE", "QTY", "VALUE", "COST", "P/L", "P/L%", "TODAY",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default().fg(Color::White)));

    let header = Row::new(header_cells)
        .style(Style::default().bg(colors.header_bg))
        .height(1);

    let rows = app.quotes.iter().enumerate().filter_map(|(i, quote)| {
        let holding = app.holdings.get(&quote.symbol)?;
        let is_selected = i == app.selected;

        let value = holding.current_value(quote.price);
        let cost = holding.total_cost();
        let pnl = holding.profit_loss(quote.price);
        let pnl_pct = holding.profit_loss_percent(quote.price);
        let today = holding.quantity * quote.change;

        let pnl_color = if pnl >= 0.0 { colors.gain } else { colors.loss };
        let today_color = if today >= 0.0 {
            colors.gain
        } else {
            colors.loss
        };

        let row_style = if is_selected {
            Style::default().bg(colors.selected_bg)
        } else {
            Style::default()
        };

        let cells = vec![
            Cell::from(quote.symbol.clone()),
            Cell::from(truncate_string(&quote.name, 15)),
            Cell::from(format_price(quote.price)),
            Cell::from(format!("{:.4}", holding.quantity)),
            Cell::from(format!("${:.2}", value)),
            Cell::from(format!("${:.2}", cost)),
            Cell::from(format!("{:+.2}", pnl)).style(Style::default().fg(pnl_color)),
            Cell::from(format!("{:+.2}%", pnl_pct)).style(Style::default().fg(pnl_color)),
            Cell::from(format!("{:+.2}", today)).style(Style::default().fg(today_color)),
        ];

        Some(Row::new(cells).style(row_style))
    });

    let widths = [
        Constraint::Length(10),
        Constraint::Length(17),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE));

    frame.render_widget(table, area);
}

/// Render the footer with keybindings.
fn render_footer(frame: &mut Frame, app: &App, area: Rect, colors: &UiColors) {
    let mode = if app.show_holdings {
        "Holdings"
    } else if app.show_fundamentals {
        "Fundamentals"
    } else {
        "Quotes"
    };
    let sort_info = format!(
        "{} {}",
        app.sort_order.header(),
        match app.sort_direction {
            crate::models::SortDirection::Ascending => "▲",
            crate::models::SortDirection::Descending => "▼",
        }
    );

    let footer = Line::from(vec![
        Span::styled(" q", Style::default().fg(Color::Yellow)),
        Span::raw(":quit "),
        Span::styled("?", Style::default().fg(Color::Yellow)),
        Span::raw(":help "),
        Span::styled("h", Style::default().fg(Color::Yellow)),
        Span::raw(":holdings "),
        Span::styled("f", Style::default().fg(Color::Yellow)),
        Span::raw(":fundamentals "),
        Span::styled("d", Style::default().fg(Color::Yellow)),
        Span::raw(":dashboard "),
        Span::styled("s", Style::default().fg(Color::Yellow)),
        Span::raw(":sort "),
        Span::styled("r", Style::default().fg(Color::Yellow)),
        Span::raw(":reverse "),
        Span::raw(format!(
            "| {} | {} | Data: {} | Iter: {}",
            mode, sort_info, app.time_since_refresh(), app.iteration
        )),
    ]);

    let footer_widget = Paragraph::new(footer).style(Style::default().bg(colors.header_bg));

    frame.render_widget(footer_widget, area);
}

/// Render help overlay.
fn render_help_overlay(frame: &mut Frame, colors: &UiColors) {
    let area = centered_rect(60, 80, frame.area());

    let help_text = vec![
        Line::from(Span::styled(
            "STONKTOP HELP",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Navigation:"),
        Line::from("  ↑/k       Move up"),
        Line::from("  ↓/j       Move down"),
        Line::from("  g/Home    Go to top"),
        Line::from("  G/End     Go to bottom"),
        Line::from("  ←/h       Previous group"),
        Line::from("  →/l       Next group"),
        Line::from("  PgUp      Page up"),
        Line::from("  PgDn      Page down"),
        Line::from(""),
        Line::from("Sorting:"),
        Line::from("  s         Cycle sort field"),
        Line::from("  r         Reverse sort order"),
        Line::from("  1-7       Sort by column"),
        Line::from(""),
        Line::from("Display:"),
        Line::from("  h         Toggle holdings view"),
        Line::from("  f         Toggle fundamentals"),
        Line::from("  d         Toggle portfolio dashboard"),
        Line::from("  Tab       Cycle groups"),
        Line::from("  /         Search (type to filter)"),
        Line::from(""),
        Line::from("Detail View (Press ENTER):"),
        Line::from("  n         Open news in browser"),
        Line::from(""),
        Line::from("Trading:"),
        Line::from("  a         Set price alert on selected stock"),
        Line::from(""),
        Line::from("Actions:"),
        Line::from("  Space/R   Force refresh"),
        Line::from("  q/Esc     Quit"),
        Line::from("  ?         Toggle help"),
        Line::from(""),
        Line::from(Span::styled(
            "ACCESSIBILITY FEATURES:",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  --high-contrast    Enhanced colors (WCAG AAA)"),
        Line::from("  --audio-alerts     Beep when alerts trigger"),
        Line::from("  --export csv|json  Export data to text formats"),
        Line::from("  --delay <secs>     Control refresh rate"),
        Line::from(""),
        Line::from("Data Freshness: Green=Fresh(0-30s), Yellow=Aging(30-60s), Red=Stale(>60s)"),
        Line::from("Trend Chart: Shows 5-bar sparkline of recent price movement"),
        Line::from(""),
        Line::from("For more info: stonktop --help or ACCESSIBILITY.md"),
        Line::from("Press any key to close"),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.border)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(Clear, area);
    frame.render_widget(help, area);
}

/// Render error message.
fn render_error(frame: &mut Frame, error: &str, colors: &UiColors) {
    let area = centered_rect(50, 20, frame.area());

    let error_widget = Paragraph::new(error)
        .block(
            Block::default()
                .title(" Error ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.loss)),
        )
        .style(Style::default().fg(colors.loss))
        .wrap(Wrap { trim: true });

    frame.render_widget(Clear, area);
    frame.render_widget(error_widget, area);
}

/// Create a centered rectangle.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Format price with appropriate precision.
/// Penny stocks get more decimals because every fraction of a cent matters
/// when you're hoping for that 10,000% gain.
fn format_price(price: f64) -> String {
    if price >= 1.0 {
        // Normal prices get normal formatting
        format!("${:.2}", price)
    } else {
        // Penny stocks and shitcoins need more precision
        format!("${:.6}", price)
    }
}

/// Format volume with suffixes.
fn format_volume(volume: u64) -> String {
    if volume >= 1_000_000_000 {
        format!("{:.2}B", volume as f64 / 1_000_000_000.0)
    } else if volume >= 1_000_000 {
        format!("{:.2}M", volume as f64 / 1_000_000.0)
    } else if volume >= 1_000 {
        format!("{:.2}K", volume as f64 / 1_000.0)
    } else {
        volume.to_formatted_string(&Locale::en)
    }
}

/// Format market cap with suffixes.
fn format_market_cap(market_cap: Option<u64>) -> String {
    match market_cap {
        Some(cap) if cap >= 1_000_000_000_000 => {
            format!("${:.2}T", cap as f64 / 1_000_000_000_000.0)
        }
        Some(cap) if cap >= 1_000_000_000 => format!("${:.2}B", cap as f64 / 1_000_000_000.0),
        Some(cap) if cap >= 1_000_000 => format!("${:.2}M", cap as f64 / 1_000_000.0),
        Some(cap) => format!("${}", cap.to_formatted_string(&Locale::en)),
        None => "-".to_string(),
    }
}

/// Truncate string to max length with ellipsis indicator.
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // Use proper ellipsis character (…) instead of "..."
        format!("{}…", &s[..max_len.saturating_sub(1)])
    }
}

/// Render batch mode output (non-interactive).
pub fn render_batch(app: &App) {
    use chrono::Local;

    println!(
        "\n=== STONKTOP {} ===",
        Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    if app.show_holdings {
        println!(
            "{:<10} {:<15} {:>10} {:>10} {:>12} {:>12} {:>10} {:>10}",
            "SYMBOL", "NAME", "PRICE", "QTY", "VALUE", "COST", "P/L", "P/L%"
        );
        println!("{}", "-".repeat(100));

        for quote in &app.quotes {
            if let Some(holding) = app.holdings.get(&quote.symbol) {
                let value = holding.current_value(quote.price);
                let cost = holding.total_cost();
                let pnl = holding.profit_loss(quote.price);
                let pnl_pct = holding.profit_loss_percent(quote.price);

                println!(
                    "{:<10} {:<15} {:>10.2} {:>10.4} {:>12.2} {:>12.2} {:>+10.2} {:>+9.2}%",
                    quote.symbol,
                    truncate_string(&quote.name, 15),
                    quote.price,
                    holding.quantity,
                    value,
                    cost,
                    pnl,
                    pnl_pct
                );
            }
        }
    } else {
        println!(
            "{:<10} {:<20} {:>12} {:>10} {:>10} {:>12} {:>12}",
            "SYMBOL", "NAME", "PRICE", "CHANGE", "CHG%", "VOLUME", "MKT CAP"
        );
        println!("{}", "-".repeat(90));

        for quote in &app.quotes {
            println!(
                "{:<10} {:<20} {:>12} {:>+10.2} {:>+9.2}% {:>12} {:>12}",
                quote.symbol,
                truncate_string(&quote.name, 20),
                format_price(quote.price),
                quote.change,
                quote.change_percent,
                format_volume(quote.volume),
                format_market_cap(quote.market_cap)
            );
        }
    }

    println!();
}

/// Get market state display string from quotes.
fn get_market_state_display(app: &App) -> String {
    if app.quotes.is_empty() {
        return "Unknown".to_string();
    }

    // Get the first non-zero market state or default to the first quote's state
    let market_state = app
        .quotes
        .first()
        .map(|q| &q.market_state)
        .unwrap_or(&crate::models::MarketState::Closed);

    match market_state {
        crate::models::MarketState::Pre => "Pre-Market".to_string(),
        crate::models::MarketState::Regular => "Market Open".to_string(),
        crate::models::MarketState::Post => "After Hours".to_string(),
        crate::models::MarketState::Closed => "Market Closed".to_string(),
    }
}

/// Render search input box.
fn render_search_input(frame: &mut Frame, app: &App, colors: &UiColors) {
    let area = centered_rect(60, 10, frame.area());

    let search_text = Line::from(vec![
        Span::raw("Search: "),
        Span::styled(
            &app.search_input,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("_"),
    ]);

    let help = Line::from(Span::styled(
        "(Enter to confirm, Esc to cancel)",
        Style::default().fg(Color::DarkGray),
    ));

    let search_widget = Paragraph::new(vec![search_text, help])
        .block(
            Block::default()
                .title(" Search ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.border)),
        );

    frame.render_widget(Clear, area);
    frame.render_widget(search_widget, area);
}

/// Render price alerts overlay.
fn render_alerts_overlay(frame: &mut Frame, app: &App, _colors: &UiColors) {
    let area = centered_rect(70, 30, frame.area());

    let mut lines = vec![
        Line::from(Span::styled(
            "⚠ PRICE ALERTS TRIGGERED",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (symbol, condition, target, current) in &app.triggered_alerts {
        let cond_str = match condition {
            crate::app::AlertCondition::Above => format!("{} > {}", symbol, target),
            crate::app::AlertCondition::Below => format!("{} < {}", symbol, target),
            crate::app::AlertCondition::Equal => format!("{} = {}", symbol, target),
        };

        lines.push(Line::from(vec![
            Span::styled(
                cond_str,
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" (now: ${:.2})", current)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press any key to dismiss",
        Style::default().fg(Color::DarkGray),
    )));

    let alerts_widget = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Alerts ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );

    frame.render_widget(Clear, area);
    frame.render_widget(alerts_widget, area);
}
/// Render alert setup modal.
fn render_alert_setup_modal(frame: &mut Frame, app: &App, symbol: &str, _colors: &UiColors) {
    let area = centered_rect(50, 20, frame.area());

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "Set Price Alert for ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                symbol,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
    ];

    if let Some((_, crate::app::AlertSetupMode::SelectCondition(idx))) = &app.alert_setup_mode {
        let conditions = vec!["Alert when price > (Above)", "Alert when price < (Below)", "Alert when price = (Equal)"];
        lines.push(Line::from("Select condition:"));
        for (i, cond) in conditions.iter().enumerate() {
            let style = if i == *idx {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(format!("  {}", cond), style)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Use ← → to select, ↓ to enter price",
            Style::default().fg(Color::DarkGray),
        )));
    } else if let Some((_, crate::app::AlertSetupMode::EnterPrice(_, price))) = &app.alert_setup_mode {
        lines.push(Line::from("Enter target price:"));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                if price.is_empty() { "_" } else { price },
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Press Enter to confirm, Esc to cancel",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let modal = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Price Alert ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );

    frame.render_widget(Clear, area);
    frame.render_widget(modal, area);
}

/// Render portfolio dashboard with P/L breakdown.
fn render_dashboard(frame: &mut Frame, app: &App, area: Rect, colors: &UiColors) {
    let mut lines = vec![
        Line::from(Span::styled(
            "PORTFOLIO DASHBOARD",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // Portfolio totals
    let total_value = app.total_portfolio_value();
    let total_pnl = app.total_portfolio_pnl();
    let total_cost = app.total_portfolio_cost();
    let pnl_pct = if total_cost > 0.0 {
        (total_pnl / total_cost) * 100.0
    } else {
        0.0
    };

    let pnl_color = if total_pnl >= 0.0 { colors.gain } else { colors.loss };

    lines.push(Line::from(vec![
        Span::raw("Total Value: "),
        Span::styled(
            format!("${:.2}", total_value),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::raw("Total P/L: "),
        Span::styled(
            format!("${:+.2} ({:+.2}%)", total_pnl, pnl_pct),
            Style::default().fg(pnl_color).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Holdings Breakdown:",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Show each holding
    for (symbol, holding) in &app.holdings {
        if let Some(quote) = app.quotes.iter().find(|q| &q.symbol == symbol) {
            let current_value = holding.current_value(quote.price);
            let cost = holding.quantity * holding.cost_basis;
            let gain = current_value - cost;
            let gain_pct = if cost > 0.0 { (gain / cost) * 100.0 } else { 0.0 };

            let gain_color = if gain >= 0.0 { colors.gain } else { colors.loss };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<10}", symbol),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(format!(
                    " {:.0} @ ${:.2} = ${:.2}  ",
                    holding.quantity, holding.cost_basis, current_value
                )),
                Span::styled(
                    format!("{:+.2} ({:+.2}%)", gain, gain_pct),
                    Style::default().fg(gain_color),
                ),
            ]));
        }
    }

    let dashboard = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors.border)),
        );

    frame.render_widget(dashboard, area);
}

/// Render detailed view for selected stock with fundamentals and market data.
fn render_detail_view(frame: &mut Frame, app: &App, area: Rect, colors: &UiColors) {
    let quotes = app.display_quotes();
    if quotes.is_empty() || app.selected >= quotes.len() {
        return;
    }

    let quote = &quotes[app.selected];
    let mut lines = vec![
        Line::from(Span::styled(
            format!("{}  {}",quote.symbol, quote.name),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    // Price info
    let price_color = if quote.change >= 0.0 {
        colors.gain
    } else {
        colors.loss
    };

    lines.push(Line::from(vec![
        Span::raw("Price: "),
        Span::styled(
            format!("${:.2}", quote.price),
            Style::default().fg(price_color).add_modifier(Modifier::BOLD),
        ),
    ]));

    lines.push(Line::from(vec![
        Span::raw("Change: "),
        Span::styled(
            format!("${:+.2} ({:+.2}%)", quote.change, quote.change_percent),
            Style::default().fg(price_color),
        ),
    ]));

    lines.push(Line::from(""));

    // Market data
    lines.push(Line::from(format!("Open: ${:.2}", quote.open)));
    lines.push(Line::from(format!("Day High: ${:.2} / Low: ${:.2}", quote.day_high, quote.day_low)));
    lines.push(Line::from(format!("52-Week High: ${:.2} / Low: ${:.2}", quote.year_high, quote.year_low)));
    lines.push(Line::from(""));

    // Volume and market cap
    lines.push(Line::from(format!("Volume: {} ({:.0}M avg)", 
        quote.volume.to_formatted_string(&Locale::en),
        quote.avg_volume as f64 / 1_000_000.0
    )));

    if let Some(market_cap) = quote.market_cap {
        lines.push(Line::from(format!("Market Cap: ${:.2}B", market_cap as f64 / 1_000_000_000.0)));
    }

    lines.push(Line::from(""));

    // Additional metadata
    lines.push(Line::from(format!("Exchange: {}", quote.exchange)));
    lines.push(Line::from(format!("Type: {}", quote.quote_type)));
    lines.push(Line::from(format!("Currency: {}", quote.currency)));
    lines.push(Line::from(""));
    
    let market_state_str = match quote.market_state {
        crate::models::MarketState::Pre => "Pre-Market",
        crate::models::MarketState::Regular => "Market Open",
        crate::models::MarketState::Post => "After Hours",
        crate::models::MarketState::Closed => "Market Closed",
    };

    lines.push(Line::from(Span::styled(
        format!("Market State: {}", market_state_str),
        Style::default().fg(Color::Yellow),
    )));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press ENTER to close, ↑↓/jk to navigate, 'n' for news",
        Style::default().fg(Color::DarkGray),
    )));

    let detail = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(colors.border))
                .title(" Stock Details ")
                .title_alignment(Alignment::Center),
        );

    frame.render_widget(detail, area);
}
