# Alert Persistence in Stonktop

## Overview

Stonktop now supports persistent price alerts that are saved to your configuration file and automatically loaded when the application starts. This feature allows you to maintain your alert settings across sessions without having to reconfigure them each time.

## How It Works

### Architecture

The alert persistence system consists of three main components:

1. **Config Serialization** (`src/config.rs`)
   - New `AlertConfig` struct stores individual alert data in TOML format
   - Alerts are stored under a `[alerts]` section in the config file
   - Each symbol maps to a vector of alert conditions and prices

2. **Loading on Startup** (`src/app.rs`)
   - When `App::new()` is called, it reads alerts from the config file via `load_alerts_from_config()`
   - Alerts are automatically populated into the in-memory `app.alerts` HashMap
   - Symbol shortcuts (e.g., "BTC") are expanded to their full forms (e.g., "BTC-USD")

3. **Saving on Demand**
   - Call `app.save_alerts_to_config()` to persist current alerts back to the config file
   - Converts enum-based AlertCondition values back to string format for serialization
   - Can be integrated into shutdown handlers or after alert creation

### Config File Format

Alerts are stored in your `~/.stonktop.toml` or specified config file:

```toml
[general]
refresh_interval = 5
timeout = 10

[watchlist]
symbols = ["AAPL", "MSFT", "GOOGL"]

[alerts]
# AAPL alerts
AAPL = [
    { condition = "above", price = 180.0 },
    { condition = "below", price = 150.0 }
]

# MSFT alerts
MSFT = [
    { condition = "above", price = 400.0 }
]

# Bitcoin alerts with shorthand (automatically expanded)
BTC = [
    { condition = "above", price = 70000.0 },
    { condition = "below", price = 60000.0 }
]
```

## Alert Conditions

Three condition types are supported:

| Condition | Meaning | Example |
|-----------|---------|---------|
| `"above"` | Price exceeds the specified value | Alert when AAPL > $180 |
| `"below"` | Price falls below the specified value | Alert when AAPL < $150 |
| `"equal"` | Price exactly equals the value | Alert when MSFT == $400 |

## Usage

### Loading Alerts

Alerts are **automatically loaded** when stonktop starts:

```bash
# Alerts from config file are loaded automatically
stonktop -s AAPL,MSFT
```

### Creating and Saving Alerts

Currently, alerts can be created programmatically or by manually editing the config file:

**Option 1: Edit Config File Directly**
```toml
[alerts]
AAPL = [{ condition = "above", price = 175.0 }]
```

**Option 2: Programmatic (in code)**
```rust
// Create an app instance
let mut app = App::new(&args, &config)?;

// Add an alert
app.add_alert("AAPL", AlertCondition::Above, 175.0);

// Save to config file
app.save_alerts_to_config(&mut config);
config.save()?;  // Write config to disk
```

## Implementation Details

### Data Structure

The in-memory representation uses:
```rust
pub alerts: HashMap<String, Vec<(AlertCondition, f64)>>
```

Each symbol maps to a vector of (condition, price) tuples.

### Serialization Format

For TOML storage, AlertCondition is converted to strings:
```rust
pub struct AlertConfig {
    pub condition: String,  // "above", "below", or "equal"
    pub price: f64,
}
```

### Symbol Expansion

When loading alerts, symbol shortcuts are automatically expanded:
- `BTC` → `BTC-USD` (cryptocurrency)
- `ETH` → `ETH-USD` (cryptocurrency)
- `AAPL` → `AAPL` (stock, no change)

This ensures consistency with the quote system.

## Example Workflow

### 1. Create Initial Config with Alerts

Create `~/.stonktop.toml`:
```toml
[general]
refresh_interval = 5

[watchlist]
symbols = ["AAPL", "BTC", "SPY"]

[alerts]
AAPL = [
    { condition = "above", price = 180.0 },
    { condition = "below", price = 150.0 }
]
BTC = [
    { condition = "above", price = 70000.0 }
]
SPY = [
    { condition = "below", price = 400.0 }
]
```

### 2. Run Stonktop

```bash
stonktop
```

The app will:
1. Load the config file
2. Load symbols from `[watchlist]` section
3. Load alerts from `[alerts]` section
4. Display quotes and check for triggered alerts

### 3. Alerts Persist Across Sessions

Each time you run stonktop, your configured alerts are automatically loaded from the config file.

## Integration with Alert Triggering

When alerts are triggered during the app's refresh cycle:
1. The triggered alert is added to `app.triggered_alerts`
2. UI displays alert notifications
3. Current alerts remain in `app.alerts` for future checking

Persistent storage ensures that even if the app crashes or is closed, the alert configuration is preserved.

## Future Enhancements

Potential improvements to the alert system:
- Interactive alert creation via UI (not yet implemented)
- Alert history/log tracking
- Alert notifications via email or webhooks
- Time-based alert filtering (market hours only, weekdays only)
- Multiple price targets per condition (e.g., "above 180 AND below 200")
- Alert groups/templates for managing multiple symbols

## Technical Notes

- **No External Dependencies**: Alert persistence uses existing `serde` and `toml` crates
- **Type Safety**: AlertCondition enum ensures only valid conditions are used in code
- **Config Isolation**: Alerts stored in separate `[alerts]` section, no conflicts with other settings
- **Backwards Compatible**: Existing configs without an `[alerts]` section work without modification (empty HashMap)

## Troubleshooting

### Alerts Not Loading

**Problem**: Config file has alerts but they don't appear in the app
**Solution**: Check that the config path is correct (use `stonktop --config <path>`) and that the `[alerts]` section format matches the examples above.

### Alert Format Error

**Problem**: Error parsing config file
**Solution**: Validate your TOML syntax:
```bash
# Use a TOML validator or check for common issues:
# - Each alert needs "condition" and "price" fields
# - Strings must be quoted: "above" not above
# - Array syntax: [{ ... }, { ... }] for multiple alerts
```

### Alerts Disappear After Restart

**Problem**: Alerts created in-memory are lost
**Solution**: Call `app.save_alerts_to_config()` before exiting to persist alerts to the config file.

