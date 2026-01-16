# TradeTerminal Technical Stack

## Architecture

**Client-Server model** with WebSocket communication for real-time order execution and position updates.

### Server (Trade Server)
- **Runtime:** Rust (stable 1.75+)
- **Purpose:** Handles Bybit API communication, order management, trailing stops
- **Deployment:** Docker container on VPS (Singapore region recommended)
- **Persistence:** In-memory state management

### Client (Trade Client)
- **Runtime:** Rust (stable 1.75+)
- **UI Framework:** Ratatui (terminal UI)
- **Purpose:** User interface, command parsing, real-time data display
- **Deployment:** Local binary or any machine with network access

## Core Technologies

### Backend
- **tokio:** Async runtime for WebSocket server and concurrent task management
- **tokio-tungstenite:** WebSocket server implementation
- **reqwest:** HTTP client for Bybit REST API
- **tokio-tungstenite (client):** WebSocket client for Bybit streaming data
- **serde/serde_json:** JSON serialization for API communication

### Frontend (TUI)
- **ratatui:** Terminal UI framework with widgets and layouts
- **crossterm:** Cross-platform terminal manipulation
- **tui-textarea:** Text input widget for command entry
- **plotters/plotters-backend-text:** ASCII chart rendering for price visualization

### Shared Libraries
- **chrono:** Timestamp handling for orders and positions
- **rust_decimal:** Precise decimal calculations for trading amounts
- **hmac/sha2:** HMAC-SHA256 for Bybit API authentication

## Communication Protocol

### WebSocket Messages (Client ↔ Server)
JSON-based command/response protocol:
- Commands: `PlaceOrder`, `CancelOrder`, `SetTrailingStop`, `GetPositions`
- Responses: `OrderPlaced`, `PositionUpdate`, `Error`

### Bybit API Integration
- **REST API:** Order placement, cancellation, position queries
- **WebSocket API:** Real-time position updates, order fills, price stream

## Development Tools

### Build & Package
- **Cargo:** Rust build system and dependency manager
- **Workspace:** Multi-crate project (client, server, shared)
- **Just:** Task runner for common commands (see `justfile`)

### Deployment
- **Docker Compose:** Container orchestration for server deployment
- **Environment Variables:** `.env` file for API credentials configuration

## Testing Strategy

### Unit Tests
- Command parsing logic
- Position size calculation
- Trailing stop activation logic

### Integration Tests
- WebSocket client-server communication
- Mock Bybit API responses

### Manual Testing
- Testnet trading with real API
- Command chaining scenarios
- UI responsiveness tests

## Observability

### Logging
- Server logs: Order execution, WebSocket events, API errors
- Client logs: Command history, connection status

### Monitoring
- Position tracking: Real-time display in TUI
- Order status: Active orders tab with updates
- Connection health: WebSocket status indicator

## Performance Considerations

### Latency Optimization
- Server on Singapore VPS: <50ms to Bybit servers
- WebSocket persistent connection: No reconnection overhead
- Async operations: Non-blocking order execution

### Resource Usage
- Server: ~10-20MB RAM (Rust minimal runtime)
- Client: ~5-10MB RAM (terminal UI only)
- Network: Minimal bandwidth (JSON messages + price stream)

## Security

### API Key Management
- Environment variables (not committed to git)
- Server-side key storage only
- HMAC-SHA256 signed requests to Bybit

### Network Security
- WebSocket over local network or VPN recommended
- No external dependencies for client
