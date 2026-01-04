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
    SetTrailingStop(TrailingStopRequest),
    PanicStop(PanicStopRequest),
    CancelPanicStop { symbol: Symbol },
    ClosePosition(ClosePositionRequest),
    GetPositions,
    GetOrders { symbol: Option<Symbol> },
    GetAccountInfo,
    GetTicker { symbol: Symbol },
    GetCandles { symbol: Symbol, interval: String, limit: u32 },
    SubscribeChart { symbol: Symbol, interval: String },
    UnsubscribeChart,
    Subscribe { symbols: Vec<Symbol> },
    Unsubscribe { symbols: Vec<Symbol> },
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrailingStopRequest {
    pub symbol: Symbol,
    pub side: crate::Side,
    pub trailing_stop: rust_decimal::Decimal,
    pub active_price: Option<rust_decimal::Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthMessage {
    pub secret_key: String,
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
    TrailingStopSet { symbol: Symbol },
    PanicStopActivated { symbol: Symbol, timeout_secs: u32, trigger_price: Option<rust_decimal::Decimal> },
    PanicStopStatus { symbol: Symbol, remaining_ms: u64, active: bool, trigger_price: Option<rust_decimal::Decimal> },
    PanicStopTriggered { symbol: Symbol },
    Positions(Vec<Position>),
    Orders(Vec<Order>),
    AccountInfo(AccountInfo),
    TickerUpdate(Ticker),
    Candles(Vec<Candle>),
    CandleUpdate(Candle),
    TradeUpdate(Trade),
    Connected,
    Disconnected,
    ChartSubscribed,
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_client_message_new() {
        let msg = ClientMessage::new(ClientPayload::Ping);
        assert!(!msg.id.is_nil());
    }

    #[test]
    fn test_client_message_unique_ids() {
        let msg1 = ClientMessage::new(ClientPayload::Ping);
        let msg2 = ClientMessage::new(ClientPayload::Ping);
        assert_ne!(msg1.id, msg2.id);
    }

    #[test]
    fn test_server_message_new() {
        let msg = ServerMessage::new(ServerPayload::Pong);
        assert!(!msg.id.is_nil());
        assert!(msg.request_id.is_none());
    }

    #[test]
    fn test_server_message_with_request_id() {
        let request_id = Uuid::new_v4();
        let msg = ServerMessage::new(ServerPayload::Pong).with_request_id(request_id);
        assert_eq!(msg.request_id, Some(request_id));
    }

    #[test]
    fn test_place_order_payload_serialization() {
        let payload = ClientPayload::PlaceOrder(OrderRequest {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Buy,
            order_type: OrderType::Market,
            quantity: dec!(0.01),
            price: None,
            time_in_force: TimeInForce::Ioc,
            reduce_only: false,
            take_profit: None,
            stop_loss: None,
            position_idx: None,
        });

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"PlaceOrder\""));
        assert!(json.contains("\"data\":{"));
    }

    #[test]
    fn test_place_order_payload_deserialization() {
        let json = r#"{"type":"PlaceOrder","data":{"symbol":"BTCUSDT","side":"buy","order_type":"limit","quantity":"0.05","price":"95000","time_in_force":"gtc","reduce_only":false}}"#;
        let payload: ClientPayload = serde_json::from_str(json).unwrap();

        match payload {
            ClientPayload::PlaceOrder(req) => {
                assert_eq!(req.symbol.0, "BTCUSDT");
                assert_eq!(req.side, Side::Buy);
                assert_eq!(req.order_type, OrderType::Limit);
                assert_eq!(req.quantity, dec!(0.05));
                assert_eq!(req.price, Some(dec!(95000)));
            }
            _ => panic!("Expected PlaceOrder"),
        }
    }

    #[test]
    fn test_cancel_order_payload() {
        let payload = ClientPayload::CancelOrder {
            order_id: "abc123".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"CancelOrder\""));
        assert!(json.contains("\"order_id\":\"abc123\""));

        let parsed: ClientPayload = serde_json::from_str(&json).unwrap();
        match parsed {
            ClientPayload::CancelOrder { order_id } => {
                assert_eq!(order_id, "abc123");
            }
            _ => panic!("Expected CancelOrder"),
        }
    }

    #[test]
    fn test_cancel_all_orders_with_symbol() {
        let payload = ClientPayload::CancelAllOrders {
            symbol: Some(Symbol::new("ETHUSDT")),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"symbol\":\"ETHUSDT\""));
    }

    #[test]
    fn test_cancel_all_orders_without_symbol() {
        let payload = ClientPayload::CancelAllOrders { symbol: None };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"symbol\":null"));
    }

    #[test]
    fn test_get_ticker_payload() {
        let payload = ClientPayload::GetTicker {
            symbol: Symbol::new("BTCUSDT"),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: ClientPayload = serde_json::from_str(&json).unwrap();

        match parsed {
            ClientPayload::GetTicker { symbol } => {
                assert_eq!(symbol.0, "BTCUSDT");
            }
            _ => panic!("Expected GetTicker"),
        }
    }

    #[test]
    fn test_subscribe_payload() {
        let payload = ClientPayload::Subscribe {
            symbols: vec![Symbol::new("BTCUSDT"), Symbol::new("ETHUSDT")],
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"Subscribe\""));

        let parsed: ClientPayload = serde_json::from_str(&json).unwrap();
        match parsed {
            ClientPayload::Subscribe { symbols } => {
                assert_eq!(symbols.len(), 2);
            }
            _ => panic!("Expected Subscribe"),
        }
    }

    #[test]
    fn test_ping_pong() {
        let ping = ClientPayload::Ping;
        let pong = ServerPayload::Pong;

        assert_eq!(serde_json::to_string(&ping).unwrap(), "{\"type\":\"Ping\"}");
        assert_eq!(serde_json::to_string(&pong).unwrap(), "{\"type\":\"Pong\"}");
    }

    #[test]
    fn test_connected_payload() {
        let payload = ServerPayload::Connected;
        let json = serde_json::to_string(&payload).unwrap();
        assert_eq!(json, "{\"type\":\"Connected\"}");
    }

    #[test]
    fn test_disconnected_payload() {
        let payload = ServerPayload::Disconnected;
        let json = serde_json::to_string(&payload).unwrap();
        assert_eq!(json, "{\"type\":\"Disconnected\"}");
    }

    #[test]
    fn test_error_payload() {
        let payload = ServerPayload::Error {
            code: 1001,
            message: "Insufficient balance".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"code\":1001"));
        assert!(json.contains("\"message\":\"Insufficient balance\""));
    }

    #[test]
    fn test_order_error_payload() {
        let payload = ServerPayload::OrderError {
            message: "Price out of range".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: ServerPayload = serde_json::from_str(&json).unwrap();

        match parsed {
            ServerPayload::OrderError { message } => {
                assert_eq!(message, "Price out of range");
            }
            _ => panic!("Expected OrderError"),
        }
    }

    #[test]
    fn test_positions_payload() {
        let payload = ServerPayload::Positions(vec![Position {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Buy,
            quantity: dec!(0.1),
            entry_price: dec!(94000),
            unrealized_pnl: dec!(100),
            leverage: 10,
            take_profit: None,
            stop_loss: None,
            trailing_stop: None,
        }]);

        let json = serde_json::to_string(&payload).unwrap();
        let parsed: ServerPayload = serde_json::from_str(&json).unwrap();

        match parsed {
            ServerPayload::Positions(positions) => {
                assert_eq!(positions.len(), 1);
                assert_eq!(positions[0].symbol.0, "BTCUSDT");
            }
            _ => panic!("Expected Positions"),
        }
    }

    #[test]
    fn test_full_client_message_roundtrip() {
        let original = ClientMessage::new(ClientPayload::PlaceOrder(OrderRequest {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Sell,
            order_type: OrderType::Limit,
            quantity: dec!(0.02),
            price: Some(dec!(96000)),
            time_in_force: TimeInForce::PostOnly,
            reduce_only: true,
            take_profit: None,
            stop_loss: None,
            position_idx: None,
        }));

        let json = serde_json::to_string(&original).unwrap();
        let parsed: ClientMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, parsed.id);
        match parsed.payload {
            ClientPayload::PlaceOrder(req) => {
                assert_eq!(req.symbol.0, "BTCUSDT");
                assert_eq!(req.side, Side::Sell);
                assert_eq!(req.price, Some(dec!(96000)));
                assert!(req.reduce_only);
            }
            _ => panic!("Expected PlaceOrder"),
        }
    }

    #[test]
    fn test_full_server_message_roundtrip() {
        let request_id = Uuid::new_v4();
        let original = ServerMessage::new(ServerPayload::OrderCancelled {
            order_id: "order-123".to_string(),
        })
        .with_request_id(request_id);

        let json = serde_json::to_string(&original).unwrap();
        let parsed: ServerMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, parsed.id);
        assert_eq!(parsed.request_id, Some(request_id));
        match parsed.payload {
            ServerPayload::OrderCancelled { order_id } => {
                assert_eq!(order_id, "order-123");
            }
            _ => panic!("Expected OrderCancelled"),
        }
    }

    #[test]
    fn test_panic_stop_activated_payload() {
        let payload = ServerPayload::PanicStopActivated {
            symbol: Symbol::new("BTCUSDT"),
            timeout_secs: 5,
            trigger_price: Some(dec!(95000)),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"PanicStopActivated\""));
        assert!(json.contains("\"symbol\":\"BTCUSDT\""));
        assert!(json.contains("\"timeout_secs\":5"));

        let parsed: ServerPayload = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerPayload::PanicStopActivated { symbol, timeout_secs, trigger_price } => {
                assert_eq!(symbol.0, "BTCUSDT");
                assert_eq!(timeout_secs, 5);
                assert_eq!(trigger_price, Some(dec!(95000)));
            }
            _ => panic!("Expected PanicStopActivated"),
        }
    }

    #[test]
    fn test_panic_stop_status_payload() {
        let payload = ServerPayload::PanicStopStatus {
            symbol: Symbol::new("ETHUSDT"),
            remaining_ms: 3500,
            active: true,
            trigger_price: Some(dec!(2500)),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"PanicStopStatus\""));
        assert!(json.contains("\"remaining_ms\":3500"));
        assert!(json.contains("\"active\":true"));

        let parsed: ServerPayload = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerPayload::PanicStopStatus { symbol, remaining_ms, active, trigger_price } => {
                assert_eq!(symbol.0, "ETHUSDT");
                assert_eq!(remaining_ms, 3500);
                assert!(active);
                assert_eq!(trigger_price, Some(dec!(2500)));
            }
            _ => panic!("Expected PanicStopStatus"),
        }
    }

    #[test]
    fn test_panic_stop_status_inactive() {
        let payload = ServerPayload::PanicStopStatus {
            symbol: Symbol::new("BTCUSDT"),
            remaining_ms: 0,
            active: false,
            trigger_price: None,
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"active\":false"));

        let parsed: ServerPayload = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerPayload::PanicStopStatus { active, .. } => {
                assert!(!active);
            }
            _ => panic!("Expected PanicStopStatus"),
        }
    }

    #[test]
    fn test_panic_stop_triggered_payload() {
        let payload = ServerPayload::PanicStopTriggered {
            symbol: Symbol::new("BTCUSDT"),
        };

        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"type\":\"PanicStopTriggered\""));
        assert!(json.contains("\"symbol\":\"BTCUSDT\""));

        let parsed: ServerPayload = serde_json::from_str(&json).unwrap();
        match parsed {
            ServerPayload::PanicStopTriggered { symbol } => {
                assert_eq!(symbol.0, "BTCUSDT");
            }
            _ => panic!("Expected PanicStopTriggered"),
        }
    }
}
