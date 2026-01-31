# Stonktop Copilot Instructions

## Project Overview
Stonktop is a top-like terminal UI for real-time stock and cryptocurrency price monitoring in Rust. It combines async data fetching (Yahoo Finance API), a TUI layer (ratatui), and interactive controls similar to `top`/`htop`. The project targets both interactive and batch modes.

**Core Architecture**: `main.rs` → `app.rs` (state) + `cli.rs`/`config.rs` (input) + `api.rs` (data) + `ui.rs` (display) + `models.rs` (types)

## Key Technical Patterns

### Data Flow
1. **Initialization**: CLI args + TOML config → `App::new()` creates state
2. **Async Refresh Loop**: `app.refresh()` calls `YahooFinanceClient::get_quotes()` with concurrency limit (12 symbols max parallel)
3. **UI Rendering**: `ui::render()` formats data using ratatui layouts and colors
4. **Batch vs Interactive**: Batch mode loops `render_batch()` without event handling; interactive mode handles keyboard events in `run_interactive()`

### API Integration Pattern
- **Yahoo Finance v8 chart endpoint**: Single symbol per request; parallelize using `tokio` + `Semaphore` with max_concurrency
- **Error Handling**: Failures collected in `app.failures` (symbol, error reason pairs); partial batch failures don't crash app
- **Symbol Expansion**: Shortcuts like `BTC` → `BTC-USD` handled by `expand_symbol()` in `api.rs`

### State Management
- `App` struct (in `app.rs`) holds all mutable state: quotes, selections, sort order, iteration counters
- Holdings are `HashMap<String, Holding>` for O(1) lookups
- Groups (watchlists) stored as `HashMap<String, Vec<String>>` from config
- UI doesn't mutate state; mutations only via keyboard handlers in `main.rs`

### Configuration
- TOML-based in `config.rs`: supports watchlists, holdings with cost basis, symbol groups, display colors
- Precedence: CLI args > TOML config > hardcoded defaults
- Default path searched: `~/.config/stonktop/config.toml` or `~/.stonktop.toml` (see `Config::default_config_path()`)

### UI Conventions
- Green = gains, Red = losses, White = neutral
- Sortable columns: symbol, price, change, change_percent, volume, market_cap
- SortDirection enum: Ascending/Descending; controlled by `app.sort_direction`
- Selected row tracked by `app.selected` index
- Overlay modals: `show_help`, `show_holdings`, `show_fundamentals` (boolean flags)

## Development Workflow

### Building & Running
```bash
cargo build --release              # Optimized binary
cargo test                         # Run tests (see tests/integration_test.rs)
cargo install --path .            # Install locally for development
stonktop -s AAPL,BTC -d 3          # Test with 3-second refresh
```

### Common Tasks
- **Add New Sort Field**: Extend `SortOrder` enum in `models.rs`, handle in `app.rs:sort_quotes()`, add CLI flag in `cli.rs`
- **Add Config Option**: Add field to struct in `config.rs` (e.g., `DisplayConfig`), merge in `App::new()`
- **Add Keyboard Command**: Add `KeyCode` match in `handle_key_event()` within `main.rs:run_interactive()`, update help text in `ui.rs`
- **API Changes**: Modify `YahooFinanceClient` in `api.rs`, update `Quote` struct in `models.rs` if schema changes

### Testing
- Integration tests in `tests/integration_test.rs`
- Focus on: configuration loading, symbol expansion, sort logic, API error resilience
- No UI tests (ratatui terminal tests are complex); test data structures and logic instead

## Project-Specific Conventions

- **Error Handling**: Use `anyhow::Result<T>` throughout; `.context("message")` for stack traces
- **Async Runtime**: All network I/O is `tokio`-based; never block in async contexts
- **Timeouts**: Configured per request (Yahoo API timeout); CLI arg `--timeout` overrides config default (30s)
- **Humor in Comments**: Code includes satirical comments about financial anxiety—preserve tone in new contributions
- **Batch Failures Resilience**: When fetching 10 symbols, if 2 fail, the other 8 are still rendered; don't exit on partial failure
- **Rust Edition**: 2021; MSRV is 1.70 (check `Cargo.toml`)

## Key Files & Responsibilities
- [src/main.rs](src/main.rs) — Entry point, event loop, mode branching
- [src/app.rs](src/app.rs) — Application state, refresh logic, sorting
- [src/api.rs](src/api.rs) — Yahoo Finance client, parallelization, symbol expansion
- [src/models.rs](src/models.rs) — Quote, Holding, SortOrder types
- [src/config.rs](src/config.rs) — TOML parsing, defaults
- [src/ui.rs](src/ui.rs) — ratatui rendering, table, colors, overlays
- [src/cli.rs](src/cli.rs) — Clap argument parser, SortField enum

## Critical Implementation Details
1. **Refresh Interval**: `app.refresh_interval` is `Duration`, converted from CLI `--delay` (seconds). Default 5s.
2. **Iteration Counting**: `app.iteration` incremented per refresh; when `app.max_iterations > 0` and reached, `app.should_quit()` returns true.
3. **Last Refresh Tracking**: `last_refresh` only updates on successful fetch; `last_refresh_attempt` always updates to prevent API hammering.
4. **Market State**: Quote contains `MarketState` enum (PRE, REGULAR, POST, CLOSED) from Yahoo API; used for future market-hours filtering.
5. **Concurrency Limit**: `max_concurrency = 12` to avoid overwhelming Yahoo's API; adjustable but not currently exposed via CLI.

## Common Pitfalls to Avoid
- **Blocking in Async**: Never use `std::thread::sleep()` in async functions; always use `tokio::time::sleep().await`
- **Mutable State**: Mutations should flow from event handlers → `App` methods, never from render functions
- **Symbol Normalization**: Always use `expand_symbol()` when user provides input; inconsistent symbol format causes missed quotes
- **Terminal State Cleanup**: The `run_interactive()` function carefully restores terminal state in a finally block—never exit without cleanup
