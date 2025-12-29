use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc;
use trade_shared::{
    ClientMessage, ClientPayload, Order, OrderRequest, OrderType, Position, ServerMessage,
    ServerPayload, Side, Symbol, TimeInForce,
};

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
                    self.messages.push(format!("Symbol: {}", self.symbol));
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
        self.messages.push("  buy <qty> [price]  - Place buy order".to_string());
        self.messages.push("  sell <qty> [price] - Place sell order".to_string());
        self.messages.push("  cancel <id>        - Cancel order".to_string());
        self.messages.push("  cancelall          - Cancel all orders".to_string());
        self.messages.push("  symbol <sym>       - Set symbol".to_string());
        self.messages.push("  positions          - Show positions".to_string());
        self.messages.push("  orders             - Show orders".to_string());
        self.messages.push("Keys: Tab=switch, r=refresh, :=command, q=quit".to_string());
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
