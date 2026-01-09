use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use trade_shared::{ClientMessage, ClientPayload, PersistedPanicStopState, ServerMessage, ServerPayload};

use crate::bybit::BybitClient;
use crate::db;
use crate::server::PanicStopState;

pub struct ClientHandler {
    bybit: Arc<BybitClient>,
    panic_stop_state: Arc<Mutex<Option<PanicStopState>>>,
    db: Arc<redb::Database>,
}

impl ClientHandler {
    pub fn new(bybit: Arc<BybitClient>, panic_stop_state: Arc<Mutex<Option<PanicStopState>>>, db: Arc<redb::Database>) -> Self {
        Self { bybit, panic_stop_state, db }
    }

    pub async fn handle(&self, msg: ClientMessage, tx: &mpsc::Sender<ServerMessage>) -> Result<()> {
        let response = match msg.payload {
            ClientPayload::PlaceOrder(req) => {
                match self.bybit.place_order(&req).await {
                    Ok(order) => ServerMessage::new(ServerPayload::OrderPlaced(order)),
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("10001") && err_str.contains("position idx not match position mode") {
                            tracing::warn!("Position mode mismatch detected, attempting to switch to Hedge mode...");
                            match self.bybit.switch_to_hedge_mode().await {
                                Ok(_) => {
                                    let notification = ServerMessage::new(ServerPayload::PositionModeChanged {
                                        from_mode: "One-Way".to_string(),
                                        to_mode: "Hedge".to_string(),
                                    });
                                    let _ = tx.send(notification).await;

                                    match self.bybit.place_order(&req).await {
                                        Ok(order) => ServerMessage::new(ServerPayload::OrderPlaced(order)),
                                        Err(retry_err) => {
                                            tracing::error!("Failed to place order after mode switch: {:#}", retry_err);
                                            ServerMessage::new(ServerPayload::OrderError {
                                                message: format!("Failed to place {:?} order for {} after switching to Hedge mode: {}", req.order_type, req.symbol, retry_err),
                                            })
                                        }
                                    }
                                }
                                Err(switch_err) => {
                                    let switch_err_str = switch_err.to_string();
                                    if switch_err_str.contains("110025") {
                                        tracing::info!("Already in hedge mode, retrying order...");
                                        match self.bybit.place_order(&req).await {
                                            Ok(order) => ServerMessage::new(ServerPayload::OrderPlaced(order)),
                                            Err(retry_err) => {
                                                tracing::error!("Failed to place order on retry: {:#}", retry_err);
                                                ServerMessage::new(ServerPayload::OrderError {
                                                    message: format!("Failed to place {:?} order for {}: {}", req.order_type, req.symbol, retry_err),
                                                })
                                            }
                                        }
                                    } else {
                                        tracing::error!("Failed to switch to hedge mode: {:#}", switch_err);
                                        ServerMessage::new(ServerPayload::OrderError {
                                            message: format!("Position mode error. Failed to switch to Hedge mode: {}", switch_err),
                                        })
                                    }
                                }
                            }
                        } else {
                            tracing::error!("Failed to place order for {}: {:#}", req.symbol, e);
                            ServerMessage::new(ServerPayload::OrderError {
                                message: format!("Failed to place {:?} order for {}: {}", req.order_type, req.symbol, e),
                            })
                        }
                    }
                }
            },

            ClientPayload::CancelOrder { order_id } => {
                match self.bybit.cancel_order(&trade_shared::Symbol::new("BTCUSDT"), &order_id).await {
                    Ok(_) => ServerMessage::new(ServerPayload::OrderCancelled { order_id }),
                    Err(e) => {
                        tracing::error!("Failed to cancel order {}: {:#}", order_id, e);
                        ServerMessage::new(ServerPayload::Error {
                            code: 1,
                            message: format!("Failed to cancel order {}: {}", order_id, e),
                        })
                    }
                }
            }

            ClientPayload::CancelAllOrders { symbol } => {
                let symbol_display = symbol.as_ref().map(|s| s.0.as_str()).unwrap_or("all symbols");
                match self.bybit.cancel_all_orders(symbol.as_ref()).await {
                    Ok(_) => ServerMessage::new(ServerPayload::OrderCancelled {
                        order_id: "all".to_string(),
                    }),
                    Err(e) => {
                        tracing::error!("Failed to cancel all orders for {}: {:#}", symbol_display, e);
                        ServerMessage::new(ServerPayload::Error {
                            code: 1,
                            message: format!("Failed to cancel all orders for {}: {}", symbol_display, e),
                        })
                    }
                }
            }

            ClientPayload::SetTrailingStop(req) => {
                match self.bybit.set_trailing_stop(&req.symbol, req.side, req.trailing_stop, req.active_price).await {
                    Ok(_) => ServerMessage::new(ServerPayload::TrailingStopSet {
                        symbol: req.symbol,
                    }),
                    Err(e) => {
                        tracing::error!("Failed to set trailing stop for {} {:?}: {:#}", req.symbol, req.side, e);
                        ServerMessage::new(ServerPayload::Error {
                            code: 1,
                            message: format!("Failed to set trailing stop for {} {:?}: {}", req.symbol, req.side, e),
                        })
                    }
                }
            }

            ClientPayload::PanicStop(req) => {
                let active = req.trigger_price.is_none();
                let start_timestamp = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let persisted = PersistedPanicStopState {
                    symbol: req.symbol.clone(),
                    side: req.side,
                    timeout_ms: req.timeout_secs as u64 * 1000,
                    trigger_price: req.trigger_price,
                    start_timestamp,
                    active,
                };
                if let Err(e) = db::save_panic_stop(&self.db, &persisted) {
                    tracing::error!("Failed to save panic stop state to database: {:#}", e);
                }
                let state = PanicStopState {
                    persisted,
                    last_trade_time: Instant::now(),
                };
                *self.panic_stop_state.lock().await = Some(state);
                if active {
                    tracing::info!("Panic stop activated immediately for {} {:?} with {}s timeout (no trigger)", req.symbol, req.side, req.timeout_secs);
                } else {
                    tracing::info!("Panic stop registered for {} {:?} with {}s timeout, trigger@{:?}", req.symbol, req.side, req.timeout_secs, req.trigger_price);
                }
                ServerMessage::new(ServerPayload::PanicStopActivated {
                    symbol: req.symbol,
                    timeout_secs: req.timeout_secs,
                    trigger_price: req.trigger_price,
                })
            }

            ClientPayload::CancelPanicStop { symbol } => {
                let mut state = self.panic_stop_state.lock().await;
                if let Some(current_state) = state.take() {
                    if let Err(e) = db::delete_panic_stop(&self.db, &current_state.persisted.symbol.0, current_state.persisted.side) {
                        tracing::error!("Failed to delete panic stop state from database: {:#}", e);
                    }
                }
                tracing::info!("Panic stop cancelled for {}", symbol);
                ServerMessage::new(ServerPayload::PanicStopStatus {
                    symbol,
                    remaining_ms: 0,
                    active: false,
                    trigger_price: None,
                })
            }

            ClientPayload::ClosePosition(req) => {
                match self.bybit.close_position(&req.symbol, req.side).await {
                    Ok(order) => ServerMessage::new(ServerPayload::OrderPlaced(order)),
                    Err(e) => {
                        tracing::error!("Failed to close position for {} {:?}: {:#}", req.symbol, req.side, e);
                        ServerMessage::new(ServerPayload::OrderError {
                            message: format!("Failed to close position for {} {:?}: {}", req.symbol, req.side, e),
                        })
                    }
                }
            }

            ClientPayload::GetPositions => match self.bybit.get_positions(None).await {
                Ok(positions) => ServerMessage::new(ServerPayload::Positions(positions)),
                Err(e) => {
                    tracing::error!("Failed to get positions: {:#}", e);
                    ServerMessage::new(ServerPayload::Error {
                        code: 1,
                        message: format!("Failed to get positions: {}", e),
                    })
                }
            },

            ClientPayload::GetOrders { symbol } => {
                let symbol_display = symbol.as_ref().map(|s| s.0.as_str()).unwrap_or("all symbols");
                match self.bybit.get_orders(symbol.as_ref()).await {
                    Ok(orders) => ServerMessage::new(ServerPayload::Orders(orders)),
                    Err(e) => {
                        tracing::error!("Failed to get orders for {}: {:#}", symbol_display, e);
                        ServerMessage::new(ServerPayload::Error {
                            code: 1,
                            message: format!("Failed to get orders for {}: {}", symbol_display, e),
                        })
                    }
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
                    Err(e) => {
                        tracing::error!("Failed to get ticker for {}: {:#}", symbol, e);
                        ServerMessage::new(ServerPayload::Error {
                            code: 1,
                            message: format!("Failed to get ticker for {}: {}", symbol, e),
                        })
                    }
                }
            }

            ClientPayload::GetCandles { symbol, interval, limit } => {
                match self.bybit.get_klines(&symbol, &interval, limit).await {
                    Ok(candles) => ServerMessage::new(ServerPayload::Candles(candles)),
                    Err(e) => {
                        tracing::error!("Failed to get candles for {} (interval={}, limit={}): {:#}", symbol, interval, limit, e);
                        ServerMessage::new(ServerPayload::Error {
                            code: 1,
                            message: format!("Failed to get candles for {}: {}", symbol, e),
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
        tx.send(response)
            .await
            .context("Failed to send response to client")?;
        Ok(())
    }
}
