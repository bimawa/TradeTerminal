use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc;
use trade_shared::{ClientMessage, ClientPayload, ServerMessage, ServerPayload};

use crate::bybit::BybitClient;

pub struct ClientHandler {
    bybit: Arc<BybitClient>,
}

impl ClientHandler {
    pub fn new(bybit: Arc<BybitClient>) -> Self {
        Self { bybit }
    }

    pub async fn handle(&self, msg: ClientMessage, tx: &mpsc::Sender<ServerMessage>) -> Result<()> {
        let response = match msg.payload {
            ClientPayload::PlaceOrder(req) => match self.bybit.place_order(&req).await {
                Ok(order) => ServerMessage::new(ServerPayload::OrderPlaced(order)),
                Err(e) => ServerMessage::new(ServerPayload::OrderError {
                    message: e.to_string(),
                }),
            },

            ClientPayload::CancelOrder { order_id } => {
                match self.bybit.cancel_order(&trade_shared::Symbol::new("BTCUSDT"), &order_id).await {
                    Ok(_) => ServerMessage::new(ServerPayload::OrderCancelled { order_id }),
                    Err(e) => ServerMessage::new(ServerPayload::Error {
                        code: 1,
                        message: e.to_string(),
                    }),
                }
            }

            ClientPayload::CancelAllOrders { symbol } => {
                match self.bybit.cancel_all_orders(symbol.as_ref()).await {
                    Ok(_) => ServerMessage::new(ServerPayload::OrderCancelled {
                        order_id: "all".to_string(),
                    }),
                    Err(e) => ServerMessage::new(ServerPayload::Error {
                        code: 1,
                        message: e.to_string(),
                    }),
                }
            }

            ClientPayload::SetTrailingStop(req) => {
                match self.bybit.set_trailing_stop(&req.symbol, req.side, req.trailing_stop, req.active_price).await {
                    Ok(_) => ServerMessage::new(ServerPayload::TrailingStopSet {
                        symbol: req.symbol,
                    }),
                    Err(e) => ServerMessage::new(ServerPayload::Error {
                        code: 1,
                        message: e.to_string(),
                    }),
                }
            }

            ClientPayload::PanicStop(req) => {
                ServerMessage::new(ServerPayload::PanicStopActivated {
                    symbol: req.symbol,
                    timeout_secs: req.timeout_secs,
                })
            }

            ClientPayload::GetPositions => match self.bybit.get_positions(None).await {
                Ok(positions) => ServerMessage::new(ServerPayload::Positions(positions)),
                Err(e) => ServerMessage::new(ServerPayload::Error {
                    code: 1,
                    message: e.to_string(),
                }),
            },

            ClientPayload::GetOrders { symbol } => {
                match self.bybit.get_orders(symbol.as_ref()).await {
                    Ok(orders) => ServerMessage::new(ServerPayload::Orders(orders)),
                    Err(e) => ServerMessage::new(ServerPayload::Error {
                        code: 1,
                        message: e.to_string(),
                    }),
                }
            }

            ClientPayload::GetAccountInfo => {
                let positions = self.bybit.get_positions(None).await.unwrap_or_default();
                ServerMessage::new(ServerPayload::AccountInfo(trade_shared::AccountInfo {
                    balances: vec![],
                    positions,
                }))
            }

            ClientPayload::GetTicker { symbol } => {
                match self.bybit.get_ticker(&symbol).await {
                    Ok(ticker) => ServerMessage::new(ServerPayload::TickerUpdate(ticker)),
                    Err(e) => ServerMessage::new(ServerPayload::Error {
                        code: 1,
                        message: e.to_string(),
                    }),
                }
            }

            ClientPayload::GetCandles { symbol, interval, limit } => {
                tracing::info!("GetCandles request: {} {} {}", symbol.0, interval, limit);
                match self.bybit.get_klines(&symbol, &interval, limit).await {
                    Ok(candles) => {
                        tracing::info!("GetCandles success: {} candles", candles.len());
                        ServerMessage::new(ServerPayload::Candles(candles))
                    }
                    Err(e) => {
                        tracing::error!("GetCandles error: {}", e);
                        ServerMessage::new(ServerPayload::Error {
                            code: 1,
                            message: e.to_string(),
                        })
                    }
                }
            }

            ClientPayload::SubscribeChart { symbol: _, interval: _ } => {
                ServerMessage::new(ServerPayload::ChartSubscribed)
            }

            ClientPayload::UnsubscribeChart => {
                ServerMessage::new(ServerPayload::ChartSubscribed)
            }

            ClientPayload::Subscribe { symbols: _ } => {
                ServerMessage::new(ServerPayload::ChartSubscribed)
            }

            ClientPayload::Unsubscribe { symbols: _ } => {
                ServerMessage::new(ServerPayload::ChartSubscribed)
            }

            ClientPayload::Ping => ServerMessage::new(ServerPayload::Pong),
        };

        let response = response.with_request_id(msg.id);
        tx.send(response).await?;
        Ok(())
    }
}
