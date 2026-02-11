use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;
use trade_shared::{Candle, Order, OrderRequest, Position, Side, Symbol, Ticker, TrailingStopTarget};

#[async_trait]
pub trait ExchangeClient: Send + Sync {
    async fn place_order(&self, req: &OrderRequest) -> Result<Order>;
    async fn cancel_order(&self, symbol: &Symbol, order_id: &str) -> Result<()>;
    async fn cancel_all_orders(&self, symbol: Option<&Symbol>) -> Result<()>;
    async fn get_orders(&self, symbol: Option<&Symbol>) -> Result<Vec<Order>>;
    async fn get_positions(&self, symbol: Option<&Symbol>) -> Result<Vec<Position>>;
    async fn close_position(&self, symbol: &Symbol, side: Side) -> Result<Order>;
    async fn get_ticker(&self, symbol: &Symbol) -> Result<Ticker>;
    async fn get_klines(&self, symbol: &Symbol, interval: &str, limit: u32) -> Result<Vec<Candle>>;
    async fn set_trailing_stop(
        &self,
        symbol: &Symbol,
        side: Side,
        target: TrailingStopTarget,
        active_price: Option<Decimal>,
    ) -> Result<()>;
}
