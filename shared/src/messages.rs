use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub id: Uuid,
    pub payload: ClientPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ClientPayload {
    PlaceOrder(OrderRequest),
    CancelOrder { order_id: String },
    CancelAllOrders { symbol: Option<Symbol> },
    GetPositions,
    GetOrders { symbol: Option<Symbol> },
    GetAccountInfo,
    GetTicker { symbol: Symbol },
    Subscribe { symbols: Vec<Symbol> },
    Unsubscribe { symbols: Vec<Symbol> },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerMessage {
    pub id: Uuid,
    pub request_id: Option<Uuid>,
    pub payload: ServerPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ServerPayload {
    OrderPlaced(Order),
    OrderCancelled { order_id: String },
    OrderUpdate(Order),
    OrderError { message: String },
    Positions(Vec<Position>),
    Orders(Vec<Order>),
    AccountInfo(AccountInfo),
    TickerUpdate(Ticker),
    Connected,
    Pong,
    Error { code: u32, message: String },
}

impl ClientMessage {
    pub fn new(payload: ClientPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            payload,
        }
    }
}

impl ServerMessage {
    pub fn new(payload: ServerPayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            request_id: None,
            payload,
        }
    }

    pub fn with_request_id(mut self, request_id: Uuid) -> Self {
        self.request_id = Some(request_id);
        self
    }
}
