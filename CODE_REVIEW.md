# Stonktop Code Review & Implementation Recommendations

## Overview
The Stonktop codebase is well-structured with clear separation of concerns, proper error handling, and good async patterns. Below are targeted recommendations organized by priority and area.

---

## 🔴 HIGH PRIORITY Issues

### 1. **Race Condition in `get_quotes()` - Semaphore Permit Bug**
**File**: [src/api.rs](src/api.rs#L63)  
**Issue**: The semaphore is acquired outside the closure but cloned inside, creating a potential race:
```rust
for symbol in symbols {
    let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
    let sym = symbol.clone();
    futs.push(async move {
        let _permit = permit;  // ← Permit held but symbol might be reused
        (sym.clone(), self.fetch_single_quote(&sym).await)
    });
}
```
The `self.fetch_single_quote()` is being called within the async block with no actual compile-time guarantee that `self` is valid across await points. While this works because `self` is behind a reference, the pattern is fragile.

**Recommendation**: Capture `self` as a single pointer reference to ensure safety:
```rust
let client = Arc::new(self.clone()); // Or refactor to use Arc<Self>
```

---

### 2. **Unbounded Memory Growth in Failure Tracking**
**File**: [src/app.rs](src/app.rs#L168)  
**Issue**: `app.failures` accumulates indefinitely:
```rust
pub failures: Vec<(String, String)>,
```
Never cleared between refreshes. With `max_concurrency=12` and many symbols, error strings can grow unbounded.

**Recommendation**: 
```rust
// In App::refresh() before fetching
self.failures.clear(); // Clear old failures
```

---

### 3. **Selection Index Out-of-Bounds on Symbol Group Switch**
**File**: [src/app.rs](src/app.rs#L127) + [src/main.rs](src/main.rs#L177)  
**Issue**: When user switches groups via Tab, `active_symbols()` returns fewer quotes but `selected` index isn't validated:
```rust
pub fn active_symbols(&self) -> Vec<String> {
    // Returns different length than self.quotes
}
```
If user selects row 5, switches group with 3 symbols, `selected` is now out-of-bounds.

**Recommendation**:
```rust
// In App::active_group assignment (needs refactoring)
pub fn set_active_group(&mut self, index: usize) {
    self.active_group = index;
    self.selected = self.selected.min(self.quotes.len().saturating_sub(1));
}
```

---

## 🟡 MEDIUM PRIORITY Issues

### 4. **No Deduplication When Merging CLI + Config Symbols**
**File**: [src/app.rs](src/app.rs#L68)  
**Issue**: Duplicates are removed, but the code clones the entire config each time:
```rust
let mut symbols: Vec<String> = args.symbols.clone()
    .unwrap_or_else(|| config.all_symbols());
```
If user provides `--symbols AAPL,BTC` AND config has `AAPL,GOOGL`, only CLI symbols are used. The deduplication happens but doesn't merge both sources.

**Expected Behavior**: Merge both lists, deduplicate. Current: CLI-only or config-only.

**Recommendation**:
```rust
let mut symbols: Vec<String> = if let Some(ref cli_symbols) = args.symbols {
    let mut merged = cli_symbols.clone();
    merged.extend(config.all_symbols());
    merged
} else {
    config.all_symbols()
};
symbols = symbols.into_iter().map(|s| expand_symbol(&s)).collect();
let mut seen = HashSet::new();
symbols.retain(|s| seen.insert(s.clone()));
```

---

### 5. **Quote Timestamp Not Reliable for Market State Determination**
**File**: [src/api.rs](src/api.rs#L285)  
**Issue**: Market state is hardcoded to `Closed`:
```rust
market_state: MarketState::Closed, // Would need separate call to determine
```
Makes `market_state` useless for filtering pre/post-market data.

**Recommendation**:
```rust
// Parse market_state from response or add to meta
let market_state = match meta.market_state {
    Some("PREPRE") => MarketState::Pre,
    Some("REGULAR") => MarketState::Regular,
    Some("POST") => MarketState::Post,
    _ => MarketState::Closed,
};
```

---

### 6. **Unbounded Error Messages in UI**
**File**: [src/main.rs](src/main.rs#L132)  
**Issue**: Error is stored but not bounded:
```rust
pub error: Option<String>,
```
Large API errors can overflow the UI. No size limit or truncation.

**Recommendation**:
```rust
pub fn set_error(&mut self, msg: &str) {
    let truncated = if msg.len() > 100 {
        format!("{}...", &msg[..97])
    } else {
        msg.to_string()
    };
    self.error = Some(truncated);
}
```

---

### 7. **No Validation of Symbol Symbols Before API Call**
**File**: [src/api.rs](src/api.rs#L87)  
**Issue**: Invalid symbols still hit Yahoo API:
```rust
pub async fn get_quotes(&self, symbols: &[String]) -> Result<QuoteBatch> {
    // No validation - directly passes to Yahoo
}
```
Wastes API quota on junk inputs like empty strings or `"..."`.

**Recommendation**:
```rust
fn is_valid_symbol(s: &str) -> bool {
    !s.is_empty() && s.len() <= 10 && 
    s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.')
}

pub async fn get_quotes(&self, symbols: &[String]) -> Result<QuoteBatch> {
    let valid: Vec<_> = symbols.iter()
        .filter(|s| is_valid_symbol(s))
        .cloned()
        .collect();
    
    if valid.len() != symbols.len() {
        let invalid: Vec<_> = symbols.iter()
            .filter(|s| !is_valid_symbol(s))
            .cloned()
            .collect();
        // Add to failures
    }
    // ...
}
```

---

### 8. **Missing Tests for Sort Logic**
**File**: [tests/integration_test.rs](tests/integration_test.rs)  
**Issue**: No unit tests for `App::sort_quotes()`. Changes to sort could regress silently.

**Recommendation**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_by_change_percent_descending() {
        let mut app = App::default();
        app.quotes = vec![
            Quote { change_percent: -1.0, .. },
            Quote { change_percent: 2.5, .. },
            Quote { change_percent: 0.0, .. },
        ];
        app.set_sort_order(SortOrder::ChangePercent);
        assert_eq!(app.quotes[0].change_percent, 2.5);
    }
}
```

---

## 🟢 LOWER PRIORITY Improvements

### 9. **Column Width Truncation in UI**
**File**: [src/ui.rs](src/ui.rs#L274)  
**Issue**: Name column truncates at 20 chars but no ellipsis indicator:
```rust
Cell::from(truncate_string(&quote.name, 20)),
```
Looks like data is missing rather than truncated.

**Recommendation**:
```rust
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len-1])  // Use … not ...
    } else {
        s.to_string()
    }
}
```

---

### 10. **Config File Search Order Not Documented**
**File**: [src/config.rs](src/config.rs) - `default_config_path()` implementation missing from read  
**Issue**: Users don't know which config paths are checked.

**Recommendation**: Add to help text and README:
```
Config file search order:
1. $STONKTOP_CONFIG environment variable
2. ./stonktop.toml (current directory)
3. ~/.stonktop.toml (home directory)
4. ~/.config/stonktop/config.toml (XDG config)
5. No config (use defaults)
```

---

### 11. **No Jitter on Refresh Interval**
**File**: [src/main.rs](src/main.rs#L70)  
**Issue**: All symbols refresh simultaneously every N seconds, hammering Yahoo API at fixed intervals.

**Recommendation**: Add small random jitter:
```rust
let jitter = std::time::Duration::from_millis(
    rand::random::<u64>() % 100
);
tokio::time::sleep(app.refresh_interval + jitter).await;
```

---

### 12. **No Caching Between Refreshes**
**File**: [src/api.rs](src/api.rs#L70)  
**Issue**: If user hits space to force refresh, API is called immediately. No deduplication if two refreshes happen within seconds.

**Recommendation**: Store last-fetch-time per-symbol:
```rust
struct CachedQuote {
    quote: Quote,
    fetched_at: Instant,
}
pub client_cache: HashMap<String, CachedQuote>,
```

---

### 13. **Terminal State Cleanup Could Panic**
**File**: [src/main.rs](src/main.rs#L100)  
**Issue**: If terminal operations fail, cleanup might not happen:
```rust
disable_raw_mode()?;  // Could fail
execute!(terminal.backend_mut(), ...)?;  // Might not reach
```

**Recommendation**: Use a guard:
```rust
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

let _guard = TerminalGuard;
// Now cleanup happens on drop even if we return early
```

---

### 14. **Iteration Counter Shows Wall-Clock Time Not Data Freshness**
**File**: [src/app.rs](src/app.rs#L164)  
**Issue**: User sees "Iter: 47" but doesn't know if that's 235 seconds or 1 second old.

**Recommendation**: Show both:
```
Iter: 47 | Last: 3s ago
```

---

### 15. **No Rate Limiting on Keypresses**
**File**: [src/main.rs](src/main.rs#L125)  
**Issue**: User can spam 'j' 100x/sec, causing selection updates to queue up.

**Recommendation**: Debounce:
```rust
let mut last_key_time = Instant::now();
if last_key_time.elapsed() < Duration::from_millis(50) {
    // Ignore rapid repeated keys
}
```

---

## 📋 Summary Table

| Issue | Severity | Category | Effort |
|-------|----------|----------|--------|
| Semaphore permit pattern | HIGH | Safety | 30min |
| Failure vec unbounded | HIGH | Memory | 10min |
| Selection OOB on group switch | HIGH | Logic | 30min |
| Symbol merging broken | MEDIUM | Config | 20min |
| Market state hardcoded | MEDIUM | Data | 20min |
| Error messages unbounded | MEDIUM | UI | 15min |
| No symbol validation | MEDIUM | Correctness | 30min |
| Missing sort tests | MEDIUM | Testing | 45min |
| Column truncation UX | LOW | UX | 10min |
| Config path docs | LOW | Docs | 10min |
| No refresh jitter | LOW | Resilience | 20min |
| No quote caching | LOW | Performance | 60min |
| Terminal cleanup panic risk | LOW | Safety | 30min |
| Iteration display | LOW | UX | 15min |
| Key debouncing | LOW | UX | 20min |

---

## Quick Win Recommendations (Next 2 Hours)

1. ✅ Add `self.failures.clear()` in `App::refresh()` - 5 min
2. ✅ Add symbol validation in `api.rs` - 20 min
3. ✅ Fix symbol merging logic in `App::new()` - 15 min
4. ✅ Add sort logic unit tests - 30 min
5. ✅ Implement `set_active_group()` with bounds check - 20 min

---

## Questions for the Author

1. Is the semaphore+closure pattern intentional or could you use a connection pool instead?
2. Do you plan to support market-hours-only filtering? (Needed for market_state)
3. Is there a reason CLI symbols override config rather than merge?
4. Have you considered a persistent cache layer for historical quotes?

