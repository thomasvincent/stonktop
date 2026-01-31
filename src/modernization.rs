//! Rust 2024 Edition Compliance
//!
//! This module documents the modernization of Stonktop to use Rust 2024 Edition patterns.
//! 
//! Key upgrades:
//! 1. Native async traits (no async-trait dependency needed)
//! 2. Parallel API orchestration with FuturesUnordered
//! 3. Terminal safety with Drop trait (TerminalGuard)
//! 4. Standardized error handling (thiserror/anyhow)
//! 5. Lifetime elision in async contexts
//! 6. Type inference improvements

#![doc = include_str!("../RUST_2024_MODERNIZATION.md")]
