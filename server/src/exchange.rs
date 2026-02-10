use anyhow::Result;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::mpsc;
use trade_shared::{Candle, Order, OrderRequest, Position, Side, Symbol, Ticker, TrailingStopTarget};

use crate::binance::{BinanceClient, BinanceWebSocket};
use crate::bybit::{BybitClient, BybitWebSocket};
use crate::config::{Config, Exchange};

pub enum ExchangeClientWrapper {
    Bybit(Arc<BybitClient>),
    Binance(Arc<BinanceClient>),
}

impl Clone for ExchangeClientWrapper {
    fn clone(&self) -> Self {
        match self {
            ExchangeClientWrapper::Bybit(c) => ExchangeClientWrapper::Bybit(c.clone()),
            ExchangeClientWrapper::Binance(c) => ExchangeClientWrapper::Binance(c.clone()),
        }
    }
}

impl ExchangeClientWrapper {
    pub fn new(config: &Config) -> Self {
        match config.exchange {
            Exchange::Bybit => ExchangeClientWrapper::Bybit(Arc::new(BybitClient::new(config))),
            Exchange::Binance => ExchangeClientWrapper::Binance(Arc::new(BinanceClient::new(config))),
        }
    }

    pub async fn place_order(&self, req: &OrderRequest) -> Result<Order> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.place_order(req).await,
            ExchangeClientWrapper::Binance(c) => c.place_order(req).await,
        }
    }

    pub async fn cancel_order(&self, symbol: &Symbol, order_id: &str) -> Result<()> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.cancel_order(symbol, order_id).await,
            ExchangeClientWrapper::Binance(c) => c.cancel_order(symbol, order_id).await,
        }
    }

    pub async fn cancel_all_orders(&self, symbol: Option<&Symbol>) -> Result<()> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.cancel_all_orders(symbol).await,
            ExchangeClientWrapper::Binance(c) => c.cancel_all_orders(symbol).await,
        }
    }

    pub async fn get_orders(&self, symbol: Option<&Symbol>) -> Result<Vec<Order>> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.get_orders(symbol).await,
            ExchangeClientWrapper::Binance(c) => c.get_orders(symbol).await,
        }
    }

    pub async fn get_positions(&self, symbol: Option<&Symbol>) -> Result<Vec<Position>> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.get_positions(symbol).await,
            ExchangeClientWrapper::Binance(c) => c.get_positions(symbol).await,
        }
    }

    pub async fn close_position(&self, symbol: &Symbol, side: Side) -> Result<Order> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.close_position(symbol, side).await,
            ExchangeClientWrapper::Binance(c) => c.close_position(symbol, side).await,
        }
    }

    pub async fn get_ticker(&self, symbol: &Symbol) -> Result<Ticker> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.get_ticker(symbol).await,
            ExchangeClientWrapper::Binance(c) => c.get_ticker(symbol).await,
        }
    }

    pub async fn get_klines(&self, symbol: &Symbol, interval: &str, limit: u32) -> Result<Vec<Candle>> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.get_klines(symbol, interval, limit).await,
            ExchangeClientWrapper::Binance(c) => c.get_klines(symbol, interval, limit).await,
        }
    }

    pub async fn set_trailing_stop(
        &self,
        symbol: &Symbol,
        side: Side,
        target: TrailingStopTarget,
        active_price: Option<Decimal>,
    ) -> Result<()> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.set_trailing_stop(symbol, side, target, active_price).await,
            ExchangeClientWrapper::Binance(c) => c.set_trailing_stop(symbol, side, target, active_price).await,
        }
    }

    pub async fn switch_to_hedge_mode(&self) -> Result<()> {
        match self {
            ExchangeClientWrapper::Bybit(c) => c.switch_to_hedge_mode().await,
            ExchangeClientWrapper::Binance(c) => c.switch_to_hedge_mode().await,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    OrderUpdate(serde_json::Value),
    PositionUpdate(serde_json::Value),
    ExecutionUpdate(serde_json::Value),
    TickerUpdate(serde_json::Value),
    KlineUpdate(serde_json::Value),
    TradeUpdate(serde_json::Value),
    Connected,
    Disconnected,
}

pub enum ExchangeWebSocketWrapper {
    Bybit(Arc<BybitWebSocket>),
    Binance(Arc<BinanceWebSocket>),
}

impl ExchangeWebSocketWrapper {
    pub fn new(config: &Config) -> Self {
        match config.exchange {
            Exchange::Bybit => ExchangeWebSocketWrapper::Bybit(Arc::new(BybitWebSocket::new(config))),
            Exchange::Binance => ExchangeWebSocketWrapper::Binance(Arc::new(BinanceWebSocket::new(config))),
        }
    }

    pub async fn connect_private(&self, tx: mpsc::Sender<WsEvent>) -> Result<()> {
        match self {
            ExchangeWebSocketWrapper::Bybit(ws) => {
                ws.connect_private(mpsc_sender_convert_bybit(tx)).await
            }
            ExchangeWebSocketWrapper::Binance(ws) => {
                ws.connect_private(mpsc_sender_convert_binance(tx)).await
            }
        }
    }

    pub async fn connect_public(&self, tx: mpsc::Sender<WsEvent>, channels: Vec<String>) -> Result<()> {
        match self {
            ExchangeWebSocketWrapper::Bybit(ws) => {
                ws.connect_public(channels, mpsc_sender_convert_bybit(tx)).await
            }
            ExchangeWebSocketWrapper::Binance(ws) => {
                ws.connect_public(mpsc_sender_convert_binance(tx), channels).await
            }
        }
    }

    pub async fn connect_chart(
        &self,
        symbol: &str,
        interval: &str,
        tx: mpsc::Sender<WsEvent>,
    ) -> Result<()> {
        match self {
            ExchangeWebSocketWrapper::Bybit(ws) => {
                ws.connect_chart(symbol, interval, mpsc_sender_convert_bybit(tx)).await
            }
            ExchangeWebSocketWrapper::Binance(ws) => {
                ws.connect_chart(symbol, interval, mpsc_sender_convert_binance(tx)).await
            }
        }
    }
}

fn mpsc_sender_convert_bybit(tx: mpsc::Sender<WsEvent>) -> mpsc::Sender<crate::bybit::ws::WsEvent> {
    let (internal_tx, mut internal_rx) = mpsc::channel(100);
    tokio::spawn(async move {
        while let Some(event) = internal_rx.recv().await {
            let converted = match event {
                crate::bybit::ws::WsEvent::OrderUpdate(v) => WsEvent::OrderUpdate(v),
                crate::bybit::ws::WsEvent::PositionUpdate(v) => WsEvent::PositionUpdate(v),
                crate::bybit::ws::WsEvent::ExecutionUpdate(v) => WsEvent::ExecutionUpdate(v),
                crate::bybit::ws::WsEvent::TickerUpdate(v) => WsEvent::TickerUpdate(v),
                crate::bybit::ws::WsEvent::KlineUpdate(v) => WsEvent::KlineUpdate(v),
                crate::bybit::ws::WsEvent::TradeUpdate(v) => WsEvent::TradeUpdate(v),
                crate::bybit::ws::WsEvent::Connected => WsEvent::Connected,
                crate::bybit::ws::WsEvent::Disconnected => WsEvent::Disconnected,
            };
            let _ = tx.send(converted).await;
        }
    });
    internal_tx
}

fn mpsc_sender_convert_binance(tx: mpsc::Sender<WsEvent>) -> mpsc::Sender<crate::binance::ws::WsEvent> {
    let (internal_tx, mut internal_rx) = mpsc::channel(100);
    tokio::spawn(async move {
        while let Some(event) = internal_rx.recv().await {
            let converted = match event {
                crate::binance::ws::WsEvent::OrderUpdate(v) => WsEvent::OrderUpdate(v),
                crate::binance::ws::WsEvent::PositionUpdate(v) => WsEvent::PositionUpdate(v),
                crate::binance::ws::WsEvent::ExecutionUpdate(v) => WsEvent::ExecutionUpdate(v),
                crate::binance::ws::WsEvent::TickerUpdate(v) => WsEvent::TickerUpdate(v),
                crate::binance::ws::WsEvent::KlineUpdate(v) => WsEvent::KlineUpdate(v),
                crate::binance::ws::WsEvent::TradeUpdate(v) => WsEvent::TradeUpdate(v),
                crate::binance::ws::WsEvent::Connected => WsEvent::Connected,
                crate::binance::ws::WsEvent::Disconnected => WsEvent::Disconnected,
            };
            let _ = tx.send(converted).await;
        }
    });
    internal_tx
}
