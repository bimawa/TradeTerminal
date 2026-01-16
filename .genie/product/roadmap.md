# TradeTerminal Roadmap

## Phase 0 — Core Functionality (✅ complete)
- Basic order placement (market/limit buy/sell)
- Position size calculation from risk amount
- Take Profit percentage-based
- Trailing Stop with activation and callback
- WebSocket client-server communication
- Docker deployment support

## Phase 1 — Enhanced Trading Features (in progress)
- **Chart Integration:** Real-time ASCII price charts with multiple timeframes
- **Sound Notifications:** Audio feedback for executed trades
- **Level Lines:** Visual price levels on charts
- **Multi-Symbol Support:** Switch between trading pairs
- **Command History:** Navigate previous commands with arrow keys

## Phase 2 — Advanced Risk Management
- **Multiple TP Levels:** Partial profit taking at different price levels
- **Dynamic Stop-Loss:** Adjust SL based on volatility or support/resistance
- **Position Scaling:** Add to winning positions with risk management
- **Risk-Reward Calculator:** Show R:R ratio before order placement
- **Max Drawdown Protection:** Automatic trading pause after loss threshold

## Phase 3 — Automation & Scripting
- **Script Mode:** Execute command sequences from files
- **Conditional Orders:** If-then order placement (e.g., if BTC > 100k then buy ETH)
- **Strategy Templates:** Predefined trading strategies (scalp, swing, etc.)
- **Backtesting:** Test command sequences on historical data
- **API for Bots:** HTTP/WebSocket API for external bot integration

## Phase 4 — Professional Features
- **Multi-Account Support:** Manage multiple Bybit accounts
- **Portfolio View:** Aggregate positions across symbols
- **Trade Journal:** Automatic logging with P&L tracking
- **Performance Analytics:** Win rate, average R:R, equity curve
- **Alert System:** Price alerts, position size alerts, risk alerts

## Phase 5 — Exchange Expansion
- **Binance Support:** Add Binance Futures connector
- **OKX Support:** Add OKX connector
- **Unified Protocol:** Abstract exchange-specific logic
- **Cross-Exchange Arbitrage:** Detect and execute arbitrage opportunities

## Success Metrics
- Order execution latency <100ms (VPS to Bybit)
- Position size calculation accuracy 100%
- Trailing stop activation rate >95%
- Zero loss of funds due to bugs
- User satisfaction: command execution <2 keystrokes average

## Dependencies & Enablers
- Stable Bybit API (no breaking changes)
- VPS availability in Singapore region
- Rust async ecosystem maturity
- Community feedback on UX improvements

## Risk Log (actively monitored)
- **API Changes:** Bybit may deprecate endpoints → mitigate with API version pinning and monitoring
- **WebSocket Disconnects:** Network issues can cause missed updates → mitigate with reconnection logic and state reconciliation
- **Fee Changes:** Exchange fee structure changes affect position sizing → mitigate with configurable fee rates
- **Order Execution Failures:** Market conditions can prevent fills → mitigate with retry logic and user notifications
