# TradeTerminal Mission

## Pitch

TradeTerminal is a professional trading terminal for Bybit with CLI interface. Client-server architecture for minimal latency when working with the exchange.

## Users

### Primary Customers

- **Active Crypto Traders:** Need fast, reliable order execution with advanced risk management
- **Algorithmic Traders:** Require scriptable CLI interface for automated trading strategies
- **Professional Traders:** Want low-latency execution from VPS close to exchange servers

### User Personas

**Day Trader**
- **Role:** Executes multiple trades daily with tight risk management
- **Context:** Needs quick order placement with automatic position sizing based on risk
- **Pain Points:** Slow web interfaces, manual position size calculations, no trailing stop automation
- **Goals:** Execute trades in <1 second, set risk once and forget calculations, automated profit protection

**Algorithmic Trader**
- **Role:** Develops and runs automated trading strategies
- **Context:** Needs scriptable CLI interface for bot integration
- **Pain Points:** Complex APIs, no pipe operators for command chaining, limited scripting support
- **Goals:** Chain commands programmatically, integrate with custom scripts, reliable WebSocket connection

**Professional Trader**
- **Role:** Trades from VPS for optimal latency
- **Context:** Requires stable server-client architecture with minimal dependencies
- **Pain Points:** Desktop apps require GUI, high latency from home connection, complex deployment
- **Goals:** Deploy server on Singapore VPS (<50ms to Bybit), lightweight client, Docker deployment

## The Problem

### Web Terminals Are Too Slow
Browser-based interfaces add latency and can't run on VPS efficiently.

**Our Approach:** CLI client connects to WebSocket server via lightweight protocol. Server runs on VPS close to exchange.

### Manual Position Sizing Is Error-Prone
Calculating position size from risk amount manually takes time and introduces mistakes.

**Our Approach:** `buyrisk/sellrisk` commands calculate position size automatically based on risk in USDT and stop-loss price, accounting for exchange fees.

### No Advanced Stop Management
Manual trailing stops require constant monitoring and updates.

**Our Approach:** Automated trailing stop with flexible activation (price or percentage) and callback configuration. Server monitors positions 24/7.

## Differentiators

### Risk-Based Position Sizing
Specify risk in USDT once - terminal calculates exact position size including fees.

### Pipe Operator for Command Chaining
Chain commands with `|` operator: `:br 10 94000 | ts 2% 0.5%` executes entry + trailing stop in one line.

### Server-Client Architecture
Run server on VPS (Singapore region) for <50ms latency to Bybit. Connect lightweight client from anywhere.

### Hedge Mode Support
Open Long and Short positions simultaneously on the same symbol.

## Key Focus Areas

- **Low Latency:** Optimized WebSocket connection, server deployment near exchange
- **Risk Management:** Automatic position sizing, trailing stops, take profit automation
- **CLI Efficiency:** Pipe operators, command shortcuts, keyboard navigation
- **Reliability:** Server runs 24/7, auto-reconnect, position monitoring
- **Ease of Deployment:** Docker Compose setup, minimal configuration
