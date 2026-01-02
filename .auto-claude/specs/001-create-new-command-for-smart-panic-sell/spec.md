# Specification: PanicStop Command - Activity-Based Market Exit

## Overview

Implement a new server-side `panicStop` (alias: `ps`) command for automated market exit when ticket/trade flow shows zero activity for a specified duration. This feature is designed for breakout trading strategies where rapid exit is critical when momentum stalls. The command accepts a timeout in seconds and an optional trigger price, monitors real-time trade activity via WebSocket, and executes a market close order when the inactivity threshold is reached. A visual timer indicator on the client's trades screen provides debug visibility for monitoring the panic stop state.

## Workflow Type

**Type**: feature

**Rationale**: This is a new end-to-end feature requiring changes across all three services (shared, server, client). It involves new protocol messages, server-side state management with real-time monitoring, and client-side UI additions. The feature has clear functional requirements and requires careful coordination between services.

## Task Scope

### Services Involved
- **server** (primary) - Handles panic stop monitoring, timer logic, and market order execution
- **shared** (integration) - New protocol messages for PanicStop command and status updates
- **client** (integration) - Command parsing, message sending, and UI timer indicator

### This Task Will:
- [ ] Add new `PanicStop` variant to `ClientPayload` in shared protocol
- [ ] Add new `PanicStopStatus` variant to `ServerPayload` for timer state updates
- [ ] Implement `panicStop`/`ps` command in client command parser
- [ ] Create server-side panic stop monitor with activity tracking
- [ ] Implement timer reset logic on new trade arrivals
- [ ] Execute market close order on timeout
- [ ] Add visual timer indicator to trades tape on chart screen
- [ ] Integrate with existing pipe system for chaining from `br`/`sr` commands

### Out of Scope:
- Persistence of panic stop state across server restarts
- Multiple concurrent panic stops on different symbols
- Historical panic stop execution logging
- Cancel/modify panic stop after activation (initial version)

## Service Context

### Server Service

**Tech Stack:**
- Language: Rust
- Framework: tokio-tungstenite, tokio async runtime
- Key directories: `server/src/`, `server/src/bybit/`

**Entry Point:** `server/src/main.rs`

**How to Run:**
```bash
just server
```

**Port:** 9000 (WebSocket)

### Shared Library

**Tech Stack:**
- Language: Rust
- Key directories: `shared/src/`

**Entry Point:** `shared/src/lib.rs`

**Key Files:**
- `shared/src/messages.rs` - Protocol messages (ClientPayload, ServerPayload)
- `shared/src/types.rs` - Core types (Order, Position, Trade, Symbol, Side)

### Client Service

**Tech Stack:**
- Language: Rust
- Framework: ratatui (TUI), tokio async
- Key directories: `client/src/`

**Entry Point:** `client/src/main.rs`

**How to Run:**
```bash
just client
```

**Key Files:**
- `client/src/app.rs` - Command parsing, COMMANDS array, pipe handling
- `client/src/ui.rs` - TUI rendering, trades tape display

## Files to Modify

| File | Service | What to Change |
|------|---------|---------------|
| `shared/src/messages.rs` | shared | Add `PanicStop` to `ClientPayload`, add `PanicStopStatus` and `PanicStopTriggered` to `ServerPayload` |
| `shared/src/types.rs` | shared | Add `PanicStopRequest` struct with symbol, side, timeout_secs, trigger_price fields |
| `client/src/app.rs` | client | Add `panicStop`/`ps` to COMMANDS, implement command parsing, handle in pipe system |
| `server/src/client_handler.rs` | server | Add handler for `PanicStop` payload |
| `server/src/server.rs` | server | Add panic stop monitor state, integrate with trade updates, timer logic |
| `client/src/ui.rs` | client | Add timer column to trades tape (draw_trades_tape function) |

## Files to Reference

These files show patterns to follow:

| File | Pattern to Copy |
|------|----------------|
| `shared/src/messages.rs` | Payload enum variants with data structures |
| `client/src/app.rs:797-866` | `set_trailing_stop()` - similar pending action with price/value parsing |
| `client/src/app.rs:441-461` | `execute_pending_action()` - pipe action execution pattern |
| `client/src/app.rs:24-41` | COMMANDS array structure and alias pattern |
| `server/src/client_handler.rs:48-58` | Handler pattern for SetTrailingStop |
| `server/src/server.rs:86-122` | Chart event handling with trade updates |
| `client/src/ui.rs:546-577` | `draw_trades_tape()` - trades display pattern |

## Patterns to Follow

### Command Definition Pattern

From `client/src/app.rs`:

```rust
const COMMANDS: &[Cmd] = &[
    Cmd { name: "buy", aliases: &["b"] },
    Cmd { name: "sell", aliases: &["s"] },
    Cmd { name: "panicStop", aliases: &["ps"] },  // NEW
    // ...
];
```

**Key Points:**
- Add to COMMANDS array with name and aliases
- Use lowercase name with camelCase convention
- Short alias should be intuitive (ps = panic stop)

### Protocol Message Pattern

From `shared/src/messages.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientPayload {
    SetTrailingStop(TrailingStopRequest),
    PanicStop(PanicStopRequest),  // NEW - follows same pattern
    // ...
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerPayload {
    TrailingStopSet { symbol: Symbol },
    PanicStopActivated { symbol: Symbol, timeout_secs: u32 },  // NEW
    PanicStopStatus { symbol: Symbol, remaining_ms: u64, active: bool },  // NEW
    PanicStopTriggered { symbol: Symbol },  // NEW
    // ...
}
```

**Key Points:**
- Use tagged enum serialization
- Create dedicated Request struct for complex payloads
- Separate payloads for different semantics (Activated, Status, Triggered)

### Pipe Integration Pattern

From `client/src/app.rs`:

```rust
async fn execute_pending_action(&mut self, pending: &PendingAction) -> Result<()> {
    let parts: Vec<&str> = pending.action.split_whitespace().collect();
    match match_command(parts[0]) {
        Some("ts") => { /* handle ts */ }
        Some("ps") => {
            // NEW: handle panic stop in pipe
            if parts.len() >= 2 {
                self.set_panic_stop(&parts[1..], Some(pending.side)).await?;
            }
        }
        _ => { /* fallback */ }
    }
}
```

**Key Points:**
- Receive side from pending action (from br/sr command)
- Parse timeout and optional trigger price from parts
- Works with pipe operator `|`

### Server-side Monitor Pattern

From `server/src/server.rs` (trade update handling):

```rust
// Pattern for monitoring trades and maintaining state
struct PanicStopState {
    symbol: Symbol,
    side: Side,
    timeout_ms: u64,
    trigger_price: Option<Decimal>,
    last_trade_time: Instant,
    active: bool,  // true when trigger price reached or no trigger price
}
```

**Key Points:**
- Maintain state per connection
- Reset timer on each TradeUpdate for matching symbol
- Use tokio interval for timer tick checking
- Execute market order when timeout expires

## Requirements

### Functional Requirements

1. **Command Parsing**
   - Description: Parse `ps <seconds> [trigger_price]` command format
   - Acceptance: Command correctly parses 1-2 arguments, validates seconds as integer, trigger_price as Decimal

2. **Pipe Integration**
   - Description: Accept piped input from `br`/`sr` commands to receive order side
   - Acceptance: `br 1 0.3% | ps 2 0.5454` correctly chains execution

3. **Activity Monitoring**
   - Description: Monitor trade stream and reset timer on each new trade for the active symbol
   - Acceptance: Timer resets to full duration when new trade arrives on WebSocket

4. **Trigger Price Logic**
   - Description: If trigger_price specified, only start counting after price reached
   - Acceptance: Timer begins only after trade price crosses trigger threshold in direction of position

5. **Market Exit Execution**
   - Description: Execute market close order when timeout expires
   - Acceptance: Reduce-only market order placed for position size on timeout

6. **Timer Status Updates**
   - Description: Send periodic status updates to client for UI display
   - Acceptance: Client receives `PanicStopStatus` every 100-200ms while active

7. **Visual Timer Indicator**
   - Description: Display countdown timer in trades tape on chart screen
   - Acceptance: Rightmost column shows remaining time, resets visually on new trade

### Edge Cases

1. **No Position Exists** - Reject panic stop if no open position for symbol/side
2. **Position Closes Before Timeout** - Cancel panic stop if position closes externally
3. **Connection Drops** - Panic stop should continue running server-side
4. **Multiple Trades in Batch** - Reset timer only once per batch, using latest trade
5. **Trigger Price Already Passed** - Start timer immediately if current price past trigger

## Implementation Notes

### DO
- Follow the pattern in `shared/src/messages.rs` for new payload variants
- Reuse `Side` and `Symbol` types from shared crate
- Use `rust_decimal::Decimal` for all price values
- Reset timer on TradeUpdate events in `server/src/server.rs`
- Send periodic status updates for smooth UI timer display
- Use reduce_only=true for the market close order
- Add the command to COMMANDS array in `client/src/app.rs`

### DON'T
- Don't reuse `TrailingStopRequest` - create dedicated `PanicStopRequest`
- Don't run timer logic on client - server-side only for latency
- Don't persist state - in-memory only for initial version
- Don't modify existing command behaviors

## Development Environment

### Start Services

```bash
# Terminal 1: Start server
just server

# Terminal 2: Start client
just client

# Or use docker
just docker-up
```

### Service URLs
- Server WebSocket: ws://127.0.0.1:9000
- Client: Terminal application (no URL)

### Required Environment Variables
- `BYBIT_API_KEY`: Bybit API key for trading
- `BYBIT_API_SECRET`: Bybit API secret for signing
- `BYBIT_TESTNET`: Set to "true" for testnet trading
- `SERVER_URL`: WebSocket URL for client (default: ws://127.0.0.1:9000)

## Success Criteria

The task is complete when:

1. [ ] `ps 2` command works standalone (panic sell after 2 sec inactivity)
2. [ ] `ps 2 0.5454` command works with trigger price
3. [ ] `br 1 0.3% | ps 2` pipe chain works correctly
4. [ ] Timer resets when new trades arrive on WebSocket
5. [ ] Market close order executes on timeout expiration
6. [ ] Timer indicator visible in trades tape on chart screen
7. [ ] No console errors during normal operation
8. [ ] Existing tests still pass (`just test`)
9. [ ] New functionality verified via manual testing

## QA Acceptance Criteria

**CRITICAL**: These criteria must be verified by the QA Agent before sign-off.

### Unit Tests
| Test | File | What to Verify |
|------|------|----------------|
| PanicStopRequest serialization | `shared/src/messages.rs` | Correct JSON serialization/deserialization |
| Command parsing | `client/src/app.rs` | `ps` and `panicStop` aliases resolve correctly |
| Value parsing | `client/src/app.rs` | Seconds and optional trigger price parse correctly |

### Integration Tests
| Test | Services | What to Verify |
|------|----------|----------------|
| PanicStop message flow | client <-> server | Message sent from client, received and handled on server |
| Status updates | server -> client | PanicStopStatus messages flow to client |
| Trade reset | bybit_ws -> server | TradeUpdate resets panic stop timer |

### End-to-End Tests
| Flow | Steps | Expected Outcome |
|------|-------|------------------|
| Basic panic stop | 1. Open position 2. `ps 5` 3. Wait 5 sec no trades | Market close executes |
| Panic stop with reset | 1. Open position 2. `ps 3` 3. Trade arrives at 2 sec | Timer resets to 3 sec |
| Pipe chain | 1. `br 1 0.3% | ps 2` 2. Position opens 3. Wait | Panic stop activates after position |
| Trigger price | 1. Open long 2. `ps 2 95000` 3. Price hits 95000 | Timer starts only at trigger |

### Browser Verification (if frontend)
| Page/Component | URL | Checks |
|----------------|-----|--------|
| Trades tape timer | Chart tab | Timer column visible, updates in real-time |
| Timer reset visual | Chart tab | Timer visually resets when new trade arrives |

### Database Verification (if applicable)
N/A - No database changes in this feature

### QA Sign-off Requirements
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] All E2E tests pass
- [ ] Timer UI verification complete
- [ ] Pipe chain integration verified
- [ ] No regressions in existing functionality
- [ ] Code follows established patterns
- [ ] No security vulnerabilities introduced
