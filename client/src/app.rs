use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc;
use trade_shared::{
    calculate_position_size, round_quantity, ClientMessage, ClientPayload, Order, OrderRequest,
    OrderType, Position, RiskError, ServerMessage, ServerPayload, Side, Symbol, TimeInForce,
};

const COMMANDS: &[&str] = &[
    "buy", "sell", "buyrisk", "sellrisk",
    "cancel", "cancelall", "symbol",
    "b", "s", "br", "sr", "c", "ca",
    "positions", "orders", "help",
];

#[derive(Debug, Clone)]
struct PendingRiskOrder {
    side: Side,
    risk_usdt: Decimal,
    sl_price: Decimal,
    symbol: String,
    tp_percent: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Orders,
    Positions,
    Trade,
}

pub struct App {
    pub input_mode: InputMode,
    pub input: String,
    pub tab: Tab,
    pub orders: Vec<Order>,
    pub positions: Vec<Position>,
    pub messages: Vec<String>,
    pub connected: bool,
    pub symbol: String,
    pub last_price: Option<Decimal>,
    conn_tx: mpsc::Sender<ClientMessage>,
    server_rx: mpsc::Receiver<ServerMessage>,
    pending_risk_order: Option<PendingRiskOrder>,
}

impl App {
    pub fn new(
        conn_tx: mpsc::Sender<ClientMessage>,
        server_rx: mpsc::Receiver<ServerMessage>,
    ) -> Self {
        Self {
            input_mode: InputMode::Normal,
            input: String::new(),
            tab: Tab::Trade,
            orders: Vec::new(),
            positions: Vec::new(),
            messages: Vec::new(),
            connected: false,
            symbol: "BTCUSDT".to_string(),
            last_price: None,
            conn_tx,
            server_rx,
            pending_risk_order: None,
        }
    }

    pub fn is_input_mode(&self) -> bool {
        self.input_mode == InputMode::Command
    }

    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.input_mode {
            InputMode::Normal => match key.code {
                KeyCode::Char(':') => {
                    self.input_mode = InputMode::Command;
                    self.input.clear();
                }
                KeyCode::Tab => {
                    self.tab = match self.tab {
                        Tab::Orders => Tab::Positions,
                        Tab::Positions => Tab::Trade,
                        Tab::Trade => Tab::Orders,
                    };
                }
                KeyCode::Char('r') => {
                    self.refresh().await?;
                }
                _ => {}
            },
            InputMode::Command => match key.code {
                KeyCode::Esc => {
                    self.input_mode = InputMode::Normal;
                    self.input.clear();
                }
                KeyCode::Enter => {
                    let cmd = self.input.clone();
                    self.input.clear();
                    self.input_mode = InputMode::Normal;
                    self.execute_command(&cmd).await?;
                }
                KeyCode::Tab => {
                    self.autocomplete();
                }
                KeyCode::Char(c) => {
                    self.input.push(c);
                }
                KeyCode::Backspace => {
                    self.input.pop();
                }
                _ => {}
            },
        }
        Ok(())
    }

    async fn execute_command(&mut self, cmd: &str) -> Result<()> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "buy" | "b" => {
                if parts.len() >= 2 {
                    self.place_order(Side::Buy, &parts[1..]).await?;
                }
            }
            "sell" | "s" => {
                if parts.len() >= 2 {
                    self.place_order(Side::Sell, &parts[1..]).await?;
                }
            }
            "cancel" | "c" => {
                if parts.len() >= 2 {
                    self.cancel_order(parts[1]).await?;
                }
            }
            "cancelall" | "ca" => {
                self.cancel_all_orders().await?;
            }
            "symbol" | "sym" => {
                if parts.len() >= 2 {
                    self.symbol = parts[1].to_uppercase();
                    self.last_price = None;
                    self.messages.push(format!("Symbol: {}", self.symbol));
                    self.refresh_ticker().await?;
                }
            }
            "positions" | "pos" => {
                self.tab = Tab::Positions;
                self.refresh_positions().await?;
            }
            "orders" | "ord" => {
                self.tab = Tab::Orders;
                self.refresh_orders().await?;
            }
            "buyrisk" | "br" => {
                if parts.len() >= 3 {
                    self.place_risk_order(Side::Buy, &parts[1..]).await?;
                } else {
                    self.messages.push("Usage: buyrisk <risk_usdt> <sl_price> [limit_price]".to_string());
                }
            }
            "sellrisk" | "sr" => {
                if parts.len() >= 3 {
                    self.place_risk_order(Side::Sell, &parts[1..]).await?;
                } else {
                    self.messages.push("Usage: sellrisk <risk_usdt> <sl_price> [limit_price]".to_string());
                }
            }
            "help" | "h" => {
                self.show_help();
            }
            _ => {
                self.messages.push(format!("Unknown command: {}", parts[0]));
            }
        }

        Ok(())
    }

    async fn place_order(&mut self, side: Side, args: &[&str]) -> Result<()> {
        let quantity = Decimal::from_str(args[0]).unwrap_or(Decimal::ZERO);
        let price = args.get(1).and_then(|p| Decimal::from_str(p).ok());

        let order_type = if price.is_some() {
            OrderType::Limit
        } else {
            OrderType::Market
        };

        let req = OrderRequest {
            symbol: Symbol::new(&self.symbol),
            side,
            order_type,
            quantity,
            price,
            time_in_force: TimeInForce::Gtc,
            reduce_only: false,
            take_profit: None,
            stop_loss: None,
        };

        let msg = ClientMessage::new(ClientPayload::PlaceOrder(req));
        self.conn_tx.send(msg).await?;
        self.messages.push(format!(
            "Placing {} {} {} @ {:?}",
            if side == Side::Buy { "BUY" } else { "SELL" },
            quantity,
            self.symbol,
            price
        ));

        Ok(())
    }

    async fn place_risk_order(&mut self, side: Side, args: &[&str]) -> Result<()> {
        let risk_usdt = match Decimal::from_str(args[0]) {
            Ok(v) => v,
            Err(_) => {
                self.messages.push("Invalid risk amount".to_string());
                return Ok(());
            }
        };

        let sl_price = match Decimal::from_str(args[1]) {
            Ok(v) => v,
            Err(_) => {
                self.messages.push("Invalid SL price".to_string());
                return Ok(());
            }
        };

        let limit_price = args.get(2).and_then(|p| {
            if p == &"-" { None } else { Decimal::from_str(p).ok() }
        });

        let tp_percent = args.get(3).and_then(|p| Decimal::from_str(p).ok());

        if let Some(entry_price) = limit_price {
            self.execute_risk_order(side, risk_usdt, sl_price, Some(entry_price), tp_percent).await?;
        } else {
            self.pending_risk_order = Some(PendingRiskOrder {
                side,
                risk_usdt,
                sl_price,
                symbol: self.symbol.clone(),
                tp_percent,
            });
            self.messages.push("Fetching price...".to_string());
            self.refresh_ticker().await?;
        }

        Ok(())
    }

    async fn execute_risk_order(
        &mut self,
        side: Side,
        risk_usdt: Decimal,
        sl_price: Decimal,
        limit_price: Option<Decimal>,
        tp_percent: Option<Decimal>,
    ) -> Result<()> {
        let is_limit = limit_price.is_some();
        let entry_price = limit_price.unwrap_or_else(|| self.last_price.unwrap_or_default());

        if entry_price.is_zero() {
            self.messages.push("No price available".to_string());
            return Ok(());
        }

        let calc = match calculate_position_size(risk_usdt, entry_price, sl_price, side, is_limit) {
            Ok(c) => c,
            Err(e) => {
                let msg = match e {
                    RiskError::InvalidRisk => "Invalid risk amount",
                    RiskError::InvalidSlPrice => "Invalid SL price",
                    RiskError::InvalidEntryPrice => "Invalid entry price",
                    RiskError::SlInvalidForSide => match side {
                        Side::Buy => "SL must be below entry price for long",
                        Side::Sell => "SL must be above entry price for short",
                    },
                    RiskError::RiskPerUnitTooSmall => "Risk per unit too small",
                };
                self.messages.push(msg.to_string());
                return Ok(());
            }
        };

        let quantity = round_quantity(calc.quantity, 0);
        let order_type = if is_limit { OrderType::Limit } else { OrderType::Market };

        let take_profit = tp_percent.map(|pct| {
            let multiplier = pct / Decimal::from(100);
            match side {
                Side::Buy => entry_price * (Decimal::ONE + multiplier),
                Side::Sell => entry_price * (Decimal::ONE - multiplier),
            }
        });

        let req = OrderRequest {
            symbol: Symbol::new(&self.symbol),
            side,
            order_type,
            quantity,
            price: limit_price,
            time_in_force: if is_limit { TimeInForce::Gtc } else { TimeInForce::Ioc },
            reduce_only: false,
            take_profit,
            stop_loss: Some(sl_price),
        };

        let msg = ClientMessage::new(ClientPayload::PlaceOrder(req));
        self.conn_tx.send(msg).await?;

        let tp_str = take_profit.map(|tp| format!(", TP: {:.2}", tp)).unwrap_or_default();
        self.messages.push(format!(
            "Risk order: {} {} {} @ {} (SL: {}{}, risk: ${}, qty: {})",
            if side == Side::Buy { "LONG" } else { "SHORT" },
            self.symbol,
            if is_limit { "LIMIT" } else { "MARKET" },
            if is_limit { entry_price.to_string() } else { format!("~{}", entry_price) },
            sl_price,
            tp_str,
            risk_usdt,
            quantity
        ));

        Ok(())
    }

    async fn cancel_order(&mut self, order_id: &str) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::CancelOrder {
            order_id: order_id.to_string(),
        });
        self.conn_tx.send(msg).await?;
        self.messages.push(format!("Cancelling order: {}", order_id));
        Ok(())
    }

    async fn cancel_all_orders(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::CancelAllOrders {
            symbol: Some(Symbol::new(&self.symbol)),
        });
        self.conn_tx.send(msg).await?;
        self.messages.push("Cancelling all orders".to_string());
        Ok(())
    }

    async fn refresh(&mut self) -> Result<()> {
        self.refresh_orders().await?;
        self.refresh_positions().await?;
        self.refresh_ticker().await?;
        Ok(())
    }

    pub async fn auto_refresh(&mut self) -> Result<()> {
        if self.connected {
            self.refresh_ticker().await?;
            self.refresh_orders().await?;
            self.refresh_positions().await?;
        }
        Ok(())
    }

    async fn refresh_ticker(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::GetTicker {
            symbol: Symbol::new(&self.symbol),
        });
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    async fn refresh_orders(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::GetOrders { symbol: None });
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    async fn refresh_positions(&mut self) -> Result<()> {
        let msg = ClientMessage::new(ClientPayload::GetPositions);
        self.conn_tx.send(msg).await?;
        Ok(())
    }

    fn show_help(&mut self) {
        self.messages.push("Commands:".to_string());
        self.messages.push("  buy <qty> [price]              - Place buy order".to_string());
        self.messages.push("  sell <qty> [price]             - Place sell order".to_string());
        self.messages.push("  buyrisk <risk$> <sl> [limit]   - Long with risk calc".to_string());
        self.messages.push("  sellrisk <risk$> <sl> [limit]  - Short with risk calc".to_string());
        self.messages.push("  cancel <id>                    - Cancel order".to_string());
        self.messages.push("  cancelall                      - Cancel all orders".to_string());
        self.messages.push("  symbol <sym>                   - Set symbol".to_string());
        self.messages.push("Keys: Tab=autocomplete, r=refresh, :=command, q=quit".to_string());
    }

    fn autocomplete(&mut self) {
        let input = self.input.trim();
        if input.is_empty() {
            return;
        }

        let matches: Vec<&str> = COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(input))
            .copied()
            .collect();

        if matches.len() == 1 {
            self.input = format!("{} ", matches[0]);
        } else if matches.len() > 1 {
            let common = Self::common_prefix(&matches);
            if common.len() > input.len() {
                self.input = common;
            }
        }
    }

    fn common_prefix(strings: &[&str]) -> String {
        if strings.is_empty() {
            return String::new();
        }
        let first = strings[0];
        let mut prefix_len = first.len();
        for s in &strings[1..] {
            prefix_len = first
                .chars()
                .zip(s.chars())
                .take_while(|(a, b)| a == b)
                .count()
                .min(prefix_len);
        }
        first[..prefix_len].to_string()
    }

    pub async fn process_messages(&mut self) -> Result<()> {
        while let Ok(msg) = self.server_rx.try_recv() {
            match msg.payload {
                ServerPayload::Connected => {
                    self.connected = true;
                    self.messages.push("Connected to server".to_string());
                    self.refresh().await?;
                }
                ServerPayload::OrderPlaced(order) => {
                    self.messages.push(format!("Order placed: {}", order.id));
                    self.refresh_orders().await?;
                }
                ServerPayload::OrderCancelled { order_id } => {
                    self.messages.push(format!("Order cancelled: {}", order_id));
                    self.refresh_orders().await?;
                }
                ServerPayload::OrderUpdate(order) => {
                    self.messages.push(format!("Order update: {} - {:?}", order.id, order.status));
                }
                ServerPayload::Orders(orders) => {
                    self.orders = orders;
                }
                ServerPayload::Positions(positions) => {
                    self.positions = positions;
                }
                ServerPayload::OrderError { message } => {
                    self.messages.push(format!("Order error: {}", message));
                }
                ServerPayload::Error { code, message } => {
                    self.messages.push(format!("Error {}: {}", code, message));
                }
                ServerPayload::TickerUpdate(ticker) => {
                    if ticker.symbol.0 == self.symbol {
                        self.last_price = Some(ticker.last_price);
                    }
                    if let Some(pending) = self.pending_risk_order.take() {
                        if ticker.symbol.0 == pending.symbol {
                            self.last_price = Some(ticker.last_price);
                            if let Err(e) = self.execute_risk_order(
                                pending.side,
                                pending.risk_usdt,
                                pending.sl_price,
                                None,
                                pending.tp_percent,
                            ).await {
                                self.messages.push(format!("Order error: {}", e));
                            }
                        } else {
                            self.pending_risk_order = Some(pending);
                        }
                    }
                }
                ServerPayload::Pong => {}
                ServerPayload::AccountInfo(_) => {}
            }
        }

        if self.messages.len() > 100 {
            self.messages.drain(0..50);
        }

        Ok(())
    }
}
