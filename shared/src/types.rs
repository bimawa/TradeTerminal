use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Symbol(pub String);

impl Symbol {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
    PostOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: Symbol,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub time_in_force: TimeInForce,
    pub reduce_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub symbol: Symbol,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub price: Option<Decimal>,
    pub average_price: Option<Decimal>,
    pub status: OrderStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: Symbol,
    pub side: Side,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub leverage: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: Symbol,
    pub last_price: Decimal,
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub volume_24h: Decimal,
    pub price_change_24h: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub available: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub balances: Vec<Balance>,
    pub positions: Vec<Position>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_symbol_new() {
        let sym = Symbol::new("BTCUSDT");
        assert_eq!(sym.0, "BTCUSDT");
    }

    #[test]
    fn test_symbol_display() {
        let sym = Symbol::new("ETHUSDT");
        assert_eq!(format!("{}", sym), "ETHUSDT");
    }

    #[test]
    fn test_symbol_equality() {
        let sym1 = Symbol::new("BTCUSDT");
        let sym2 = Symbol::new("BTCUSDT");
        let sym3 = Symbol::new("ETHUSDT");
        assert_eq!(sym1, sym2);
        assert_ne!(sym1, sym3);
    }

    #[test]
    fn test_symbol_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Symbol::new("BTCUSDT"));
        set.insert(Symbol::new("BTCUSDT"));
        set.insert(Symbol::new("ETHUSDT"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_side_serialization() {
        assert_eq!(serde_json::to_string(&Side::Buy).unwrap(), "\"buy\"");
        assert_eq!(serde_json::to_string(&Side::Sell).unwrap(), "\"sell\"");
    }

    #[test]
    fn test_side_deserialization() {
        assert_eq!(serde_json::from_str::<Side>("\"buy\"").unwrap(), Side::Buy);
        assert_eq!(serde_json::from_str::<Side>("\"sell\"").unwrap(), Side::Sell);
    }

    #[test]
    fn test_order_type_serialization() {
        assert_eq!(serde_json::to_string(&OrderType::Market).unwrap(), "\"market\"");
        assert_eq!(serde_json::to_string(&OrderType::Limit).unwrap(), "\"limit\"");
    }

    #[test]
    fn test_order_status_serialization() {
        assert_eq!(serde_json::to_string(&OrderStatus::New).unwrap(), "\"new\"");
        assert_eq!(serde_json::to_string(&OrderStatus::Filled).unwrap(), "\"filled\"");
        assert_eq!(serde_json::to_string(&OrderStatus::Cancelled).unwrap(), "\"cancelled\"");
    }

    #[test]
    fn test_time_in_force_serialization() {
        assert_eq!(serde_json::to_string(&TimeInForce::Gtc).unwrap(), "\"gtc\"");
        assert_eq!(serde_json::to_string(&TimeInForce::Ioc).unwrap(), "\"ioc\"");
        assert_eq!(serde_json::to_string(&TimeInForce::Fok).unwrap(), "\"fok\"");
        assert_eq!(serde_json::to_string(&TimeInForce::PostOnly).unwrap(), "\"postonly\"");
    }

    #[test]
    fn test_order_request_serialization() {
        let req = OrderRequest {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Buy,
            order_type: OrderType::Limit,
            quantity: dec!(0.01),
            price: Some(dec!(95000)),
            time_in_force: TimeInForce::Gtc,
            reduce_only: false,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"symbol\":\"BTCUSDT\""));
        assert!(json.contains("\"side\":\"buy\""));
        assert!(json.contains("\"order_type\":\"limit\""));
        assert!(json.contains("\"quantity\":\"0.01\""));
        assert!(json.contains("\"price\":\"95000\""));
    }

    #[test]
    fn test_order_request_market_no_price() {
        let req = OrderRequest {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Sell,
            order_type: OrderType::Market,
            quantity: dec!(0.05),
            price: None,
            time_in_force: TimeInForce::Ioc,
            reduce_only: true,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"price\":null"));
        assert!(json.contains("\"reduce_only\":true"));
    }

    #[test]
    fn test_position_serialization() {
        let pos = Position {
            symbol: Symbol::new("BTCUSDT"),
            side: Side::Buy,
            quantity: dec!(0.1),
            entry_price: dec!(94500),
            unrealized_pnl: dec!(50.25),
            leverage: 10,
        };

        let json = serde_json::to_string(&pos).unwrap();
        let parsed: Position = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.symbol.0, "BTCUSDT");
        assert_eq!(parsed.quantity, dec!(0.1));
        assert_eq!(parsed.leverage, 10);
    }

    #[test]
    fn test_ticker_serialization() {
        let ticker = Ticker {
            symbol: Symbol::new("BTCUSDT"),
            last_price: dec!(95000),
            bid_price: dec!(94999),
            ask_price: dec!(95001),
            volume_24h: dec!(1000000),
            price_change_24h: dec!(0.025),
        };

        let json = serde_json::to_string(&ticker).unwrap();
        let parsed: Ticker = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.last_price, dec!(95000));
        assert_eq!(parsed.bid_price, dec!(94999));
        assert_eq!(parsed.ask_price, dec!(95001));
    }

    #[test]
    fn test_balance_serialization() {
        let balance = Balance {
            asset: "USDT".to_string(),
            available: dec!(1000.50),
            locked: dec!(100.25),
        };

        let json = serde_json::to_string(&balance).unwrap();
        let parsed: Balance = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.asset, "USDT");
        assert_eq!(parsed.available, dec!(1000.50));
        assert_eq!(parsed.locked, dec!(100.25));
    }

    #[test]
    fn test_account_info_serialization() {
        let info = AccountInfo {
            balances: vec![Balance {
                asset: "USDT".to_string(),
                available: dec!(5000),
                locked: dec!(0),
            }],
            positions: vec![Position {
                symbol: Symbol::new("BTCUSDT"),
                side: Side::Buy,
                quantity: dec!(0.01),
                entry_price: dec!(94000),
                unrealized_pnl: dec!(10),
                leverage: 5,
            }],
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: AccountInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.balances.len(), 1);
        assert_eq!(parsed.positions.len(), 1);
        assert_eq!(parsed.balances[0].asset, "USDT");
        assert_eq!(parsed.positions[0].symbol.0, "BTCUSDT");
    }
}
