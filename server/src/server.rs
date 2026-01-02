use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use trade_shared::{ClientMessage, ClientPayload, OrderRequest, OrderType, ServerMessage, ServerPayload, Side, Symbol, TimeInForce, Trade};

pub struct PanicStopState {
    pub symbol: Symbol,
    pub side: Side,
    pub timeout_ms: u64,
    pub trigger_price: Option<Decimal>,
    pub last_trade_time: Instant,
    pub active: bool,
}

use crate::bybit::{BybitClient, BybitWebSocket, WsEvent};
use crate::client_handler::ClientHandler;
use crate::config::Config;

pub async fn run(config: Config) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr).await?;
    let bybit_client = Arc::new(BybitClient::new(&config));
    let bybit_ws = Arc::new(BybitWebSocket::new(&config));

    tracing::info!("Server listening on {}", config.listen_addr);

    while let Ok((stream, addr)) = listener.accept().await {
        tracing::info!("New connection from {}", addr);
        let bybit = bybit_client.clone();
        let ws = bybit_ws.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, bybit, ws).await {
                tracing::error!("Connection error: {}", e);
            }
        });
    }

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    bybit: Arc<BybitClient>,
    bybit_ws: Arc<BybitWebSocket>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut write, mut read) = ws_stream.split();

    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);
    let bybit_for_timer = bybit.clone();
    let panic_stop_state: Arc<Mutex<Option<PanicStopState>>> = Arc::new(Mutex::new(None));
    let handler = ClientHandler::new(bybit, panic_stop_state.clone());

    let connected_msg = ServerMessage::new(ServerPayload::Connected);
    write
        .send(Message::Text(serde_json::to_string(&connected_msg)?))
        .await?;

    let (ws_event_tx, mut ws_event_rx) = mpsc::channel(100);
    let ws_clone = bybit_ws.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_clone.connect_private(ws_event_tx).await {
            tracing::error!("Bybit WS error: {}", e);
        }
    });

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        while let Some(event) = ws_event_rx.recv().await {
            let msg = match event {
                WsEvent::OrderUpdate(data) => {
                    if let Ok(orders) = serde_json::from_value(data) {
                        Some(ServerMessage::new(ServerPayload::Orders(orders)))
                    } else {
                        None
                    }
                }
                WsEvent::PositionUpdate(data) => {
                    if let Ok(positions) = serde_json::from_value(data) {
                        Some(ServerMessage::new(ServerPayload::Positions(positions)))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(msg) = msg {
                let _ = tx_clone.send(msg).await;
            }
        }
    });

    let chart_symbol: Arc<Mutex<Option<Symbol>>> = Arc::new(Mutex::new(None));

    let (chart_event_tx, mut chart_event_rx) = mpsc::channel::<WsEvent>(100);
    let chart_tx = tx.clone();
    let panic_stop_for_chart = panic_stop_state.clone();
    let chart_symbol_for_handler = chart_symbol.clone();
    tokio::spawn(async move {
        while let Some(event) = chart_event_rx.recv().await {
            let msg = match event {
                WsEvent::TradeUpdate(data) => {
                    let mut last_trade_price: Option<Decimal> = None;
                    if let Some(trades) = data.as_array() {
                        for trade_data in trades {
                            if let Some(trade) = parse_trade(trade_data) {
                                last_trade_price = Some(trade.price);
                                let _ = chart_tx.send(ServerMessage::new(ServerPayload::TradeUpdate(trade))).await;
                            }
                        }
                    }
                    if let Some(trade_price) = last_trade_price {
                        let subscribed_symbol = chart_symbol_for_handler.lock().await.clone();
                        let mut state_guard = panic_stop_for_chart.lock().await;
                        if let Some(ref mut state) = *state_guard {
                            if subscribed_symbol.as_ref() == Some(&state.symbol) {
                                if let Some(trigger) = state.trigger_price {
                                    let triggered = match state.side {
                                        Side::Buy => trade_price >= trigger,
                                        Side::Sell => trade_price <= trigger,
                                    };
                                    if triggered && !state.active {
                                        state.active = true;
                                        state.last_trade_time = Instant::now();
                                        tracing::info!("Panic stop activated for {} at trigger price {}", state.symbol, trigger);
                                    }
                                }
                                if state.active {
                                    state.last_trade_time = Instant::now();
                                }
                            }
                        }
                    }
                    None
                }
                WsEvent::KlineUpdate(data) => {
                    if let Some(candles) = data.as_array() {
                        if let Some(candle_data) = candles.first() {
                            if let Some(candle) = parse_candle(candle_data) {
                                Some(ServerMessage::new(ServerPayload::CandleUpdate(candle)))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(msg) = msg {
                let _ = chart_tx.send(msg).await;
            }
        }
    });

    let mut chart_subscription: Option<tokio::task::JoinHandle<()>> = None;

    let write_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(text) = serde_json::to_string(&msg) {
                if write.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut timer_interval = tokio::time::interval(Duration::from_millis(100));
    let panic_stop_for_timer = panic_stop_state.clone();
    let tx_for_timer = tx.clone();

    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                if let ClientPayload::SubscribeChart { ref symbol, ref interval } = client_msg.payload {
                                    if let Some(handle) = chart_subscription.take() {
                                        handle.abort();
                                    }
                                    *chart_symbol.lock().await = Some(symbol.clone());
                                    let ws = bybit_ws.clone();
                                    let sym = symbol.0.clone();
                                    let int = interval.clone();
                                    let evt_tx = chart_event_tx.clone();
                                    chart_subscription = Some(tokio::spawn(async move {
                                        tracing::info!("Starting chart subscription for {} {}", sym, int);
                                        if let Err(e) = ws.connect_chart(&sym, &int, evt_tx).await {
                                            tracing::error!("Chart WS error: {}", e);
                                        }
                                    }));
                                    let response = ServerMessage::new(ServerPayload::ChartSubscribed).with_request_id(client_msg.id);
                                    let _ = tx.send(response).await;
                                } else if let ClientPayload::UnsubscribeChart = client_msg.payload {
                                    if let Some(handle) = chart_subscription.take() {
                                        handle.abort();
                                    }
                                    *chart_symbol.lock().await = None;
                                    let response = ServerMessage::new(ServerPayload::ChartSubscribed).with_request_id(client_msg.id);
                                    let _ = tx.send(response).await;
                                } else {
                                    if let Err(e) = handler.handle(client_msg, &tx).await {
                                        tracing::error!("Handler error: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Invalid message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => break,
                    Some(Err(e)) => {
                        tracing::error!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
            _ = timer_interval.tick() => {
                let mut state_guard = panic_stop_for_timer.lock().await;
                if let Some(ref mut state) = *state_guard {
                    if state.active {
                        let elapsed = state.last_trade_time.elapsed().as_millis() as u64;
                        let remaining_ms = state.timeout_ms.saturating_sub(elapsed);

                        let status_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                            symbol: state.symbol.clone(),
                            remaining_ms,
                            active: true,
                        });
                        let _ = tx_for_timer.send(status_msg).await;

                        if elapsed >= state.timeout_ms {
                            tracing::info!("Panic stop timeout reached for {}, executing market close", state.symbol);

                            let close_side = match state.side {
                                Side::Buy => Side::Sell,
                                Side::Sell => Side::Buy,
                            };

                            if let Ok(positions) = bybit_for_timer.get_positions(Some(&state.symbol)).await {
                                if let Some(position) = positions.iter().find(|p| p.symbol == state.symbol && p.side == state.side) {
                                    let order_req = OrderRequest {
                                        symbol: state.symbol.clone(),
                                        side: close_side,
                                        order_type: OrderType::Market,
                                        quantity: position.quantity,
                                        price: None,
                                        time_in_force: TimeInForce::Ioc,
                                        reduce_only: true,
                                        take_profit: None,
                                        stop_loss: None,
                                    };

                                    match bybit_for_timer.place_order(&order_req).await {
                                        Ok(_) => {
                                            tracing::info!("Panic stop market close executed for {}", state.symbol);
                                            let triggered_msg = ServerMessage::new(ServerPayload::PanicStopTriggered {
                                                symbol: state.symbol.clone(),
                                            });
                                            let _ = tx_for_timer.send(triggered_msg).await;
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to execute panic stop market close: {}", e);
                                        }
                                    }
                                } else {
                                    tracing::warn!("Position not found for panic stop: {} {:?}", state.symbol, state.side);
                                }
                            } else {
                                tracing::error!("Failed to get positions for panic stop");
                            }

                            *state_guard = None;
                        }
                    } else if state.trigger_price.is_none() {
                        state.active = true;
                        state.last_trade_time = Instant::now();
                        tracing::info!("Panic stop activated immediately for {} (no trigger price)", state.symbol);
                    }
                }
            }
        }
    }

    if let Some(handle) = chart_subscription.take() {
        handle.abort();
    }

    write_task.abort();
    Ok(())
}

fn parse_trade(data: &serde_json::Value) -> Option<Trade> {
    let timestamp = data.get("T")?.as_i64()?;
    let price = Decimal::from_str(data.get("p")?.as_str()?).ok()?;
    let qty = Decimal::from_str(data.get("v")?.as_str()?).ok()?;
    let side_str = data.get("S")?.as_str()?;
    let side = if side_str == "Buy" { Side::Buy } else { Side::Sell };
    
    Some(Trade {
        timestamp,
        price,
        qty,
        side,
    })
}

fn parse_candle(data: &serde_json::Value) -> Option<trade_shared::Candle> {
    let start = data.get("start")?.as_i64()?;
    let open = Decimal::from_str(data.get("open")?.as_str()?).ok()?;
    let high = Decimal::from_str(data.get("high")?.as_str()?).ok()?;
    let low = Decimal::from_str(data.get("low")?.as_str()?).ok()?;
    let close = Decimal::from_str(data.get("close")?.as_str()?).ok()?;
    let volume = Decimal::from_str(data.get("volume")?.as_str()?).ok()?;
    
    Some(trade_shared::Candle {
        timestamp: start,
        open,
        high,
        low,
        close,
        volume,
    })
}
