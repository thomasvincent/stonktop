# stonktop

## Purpose
Terminal UI (TUI) for monitoring stock and cryptocurrency prices in real-time, like `top` for stonks.

## Stack
- **Language:** Rust (Edition 2024, MSRV 1.85)
- **TUI:** ratatui 0.30 + crossterm 0.29
- **Async:** Tokio (full features)
- **HTTP:** reqwest (JSON)
- **Serialization:** serde + serde_json

## Commands
```bash
cargo build              # Build
cargo test               # Run tests
cargo run                # Run the TUI
cargo clippy             # Lint
cargo fmt                # Format
```

## Conventions
- Rust 2024 edition idioms
- `clippy` pedantic clean
- `Result`/`Option` for all fallible operations
- Structured error handling with `?` operator
- Async/await with Tokio runtime
- Decimal types for financial values
- Rate limiting for API calls
