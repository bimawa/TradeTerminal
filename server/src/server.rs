use anyhow::{Context, Result};
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
    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .context(format!("Failed to bind to {}", config.listen_addr))?;
    let bybit_client = Arc::new(BybitClient::new(&config));
    let bybit_ws = Arc::new(BybitWebSocket::new(&config));

    tracing::info!("Server listening on {}", config.listen_addr);

    while let Ok((stream, addr)) = listener.accept().await {
        tracing::info!("New connection from {}", addr);
        let bybit = bybit_client.clone();
        let ws = bybit_ws.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, bybit, ws).await {
                tracing::error!(addr = %addr, error = %e, "Connection error");
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
    let ws_stream = accept_async(stream)
        .await
        .context("Failed to accept WebSocket connection")?;
    let (mut write, mut read) = ws_stream.split();

    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);
    let bybit_for_timer = bybit.clone();
    let panic_stop_state: Arc<Mutex<Option<PanicStopState>>> = Arc::new(Mutex::new(None));
    let handler = ClientHandler::new(bybit, panic_stop_state.clone());

    let connected_msg = ServerMessage::new(ServerPayload::Connected);
    let connected_json = serde_json::to_string(&connected_msg)
        .context("Failed to serialize Connected message")?;
    write
        .send(Message::Text(connected_json))
        .await
        .context("Failed to send Connected message")?;

    let (ws_event_tx, mut ws_event_rx) = mpsc::channel(100);
    let ws_clone = bybit_ws.clone();
    tokio::spawn(async move {
        if let Err(e) = ws_clone.connect_private(ws_event_tx).await {
            tracing::error!(error = %e, "Bybit private WebSocket connection failed");
        }
    });

    let tx_clone = tx.clone();
    let panic_stop_for_ws = panic_stop_state.clone();
    let ws_event_task = tokio::spawn(async move {
        while let Some(event) = ws_event_rx.recv().await {
            let msg = match event {
                WsEvent::OrderUpdate(data) => {
                    match serde_json::from_value(data.clone()) {
                        Ok(orders) => Some(ServerMessage::new(ServerPayload::Orders(orders))),
                        Err(e) => {
                            tracing::warn!(error = %e, data = ?data, "Failed to parse order update");
                            None
                        }
                    }
                }
                WsEvent::PositionUpdate(data) => {
                    match serde_json::from_value::<Vec<trade_shared::Position>>(data.clone()) {
                        Ok(positions) => {
                            let mut state_guard = panic_stop_for_ws.lock().await;
                            if let Some(ref state) = *state_guard {
                                let position_exists = positions.iter().any(|p| {
                                    p.symbol == state.symbol && p.side == state.side && p.quantity > rust_decimal::Decimal::ZERO
                                });
                                if !position_exists {
                                    tracing::info!(
                                        symbol = %state.symbol,
                                        side = ?state.side,
                                        "Panic stop canceled: position closed"
                                    );
                                    let symbol_clone = state.symbol.clone();
                                    *state_guard = None;
                                    let cancel_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                                        symbol: symbol_clone,
                                        remaining_ms: 0,
                                        active: false,
                                    });
                                    let _ = tx_clone.send(cancel_msg).await;
                                }
                            }
                            Some(ServerMessage::new(ServerPayload::Positions(positions)))
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, data = ?data, "Failed to parse position update");
                            None
                        }
                    }
                }
                _ => None,
            };
            if let Some(msg) = msg {
                if tx_clone.send(msg).await.is_err() {
                    break;
                }
            }
        }
    });

    let chart_symbol: Arc<Mutex<Option<Symbol>>> = Arc::new(Mutex::new(None));

    let (chart_event_tx, mut chart_event_rx) = mpsc::channel::<WsEvent>(100);
    let chart_tx = tx.clone();
    let panic_stop_for_chart = panic_stop_state.clone();
    let chart_symbol_for_handler = chart_symbol.clone();
    let chart_event_task = tokio::spawn(async move {
        while let Some(event) = chart_event_rx.recv().await {
            let msg = match event {
                WsEvent::TradeUpdate(data) => {
                    let mut last_trade_price: Option<Decimal> = None;
                    if let Some(trades) = data.as_array() {
                        for trade_data in trades {
                            if let Some(trade) = parse_trade(trade_data) {
                                last_trade_price = Some(trade.price);
                                if chart_tx.send(ServerMessage::new(ServerPayload::TradeUpdate(trade))).await.is_err() {
                                    return;
                                }
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
                                        tracing::info!(symbol = %state.symbol, trigger_price = %trigger, trade_price = %trade_price, side = ?state.side, "Panic stop activated at trigger price");
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
                            parse_candle(candle_data).map(|candle| ServerMessage::new(ServerPayload::CandleUpdate(candle)))
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
                if chart_tx.send(msg).await.is_err() {
                    break;
                }
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
                                        tracing::info!(symbol = %sym, interval = %int, "Starting chart subscription");
                                        if let Err(e) = ws.connect_chart(&sym, &int, evt_tx).await {
                                            tracing::error!(symbol = %sym, interval = %int, error = %e, "Chart WebSocket connection failed");
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
                                } else if let Err(e) = handler.handle(client_msg.clone(), &tx).await {
                                    tracing::error!(payload = ?client_msg.payload, error = %e, "Client handler error");
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, message = %text, "Failed to parse client message");
                            }
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        tracing::debug!(frame = ?frame, "Client closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!(error = %e, "WebSocket read error");
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
                            tracing::info!(symbol = %state.symbol, side = ?state.side, timeout_ms = state.timeout_ms, "Panic stop timeout reached, executing market close");

                            let close_side = match state.side {
                                Side::Buy => Side::Sell,
                                Side::Sell => Side::Buy,
                            };

                            match bybit_for_timer.get_positions(Some(&state.symbol)).await {
                                Ok(positions) => {
                                    if let Some(position) = positions.iter().find(|p| p.symbol == state.symbol && p.side == state.side && p.quantity > rust_decimal::Decimal::ZERO) {
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
                                                tracing::info!(symbol = %state.symbol, side = ?state.side, qty = %position.quantity, "Panic stop market close executed");
                                                let symbol_clone = state.symbol.clone();
                                                *state_guard = None;
                                                let triggered_msg = ServerMessage::new(ServerPayload::PanicStopTriggered {
                                                    symbol: symbol_clone,
                                                });
                                                let _ = tx_for_timer.send(triggered_msg).await;
                                            }
                                            Err(e) => {
                                                tracing::error!(symbol = %state.symbol, side = ?state.side, error = %e, "Failed to execute panic stop market close");
                                                let symbol_clone = state.symbol.clone();
                                                *state_guard = None;
                                                let cancel_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                                                    symbol: symbol_clone,
                                                    remaining_ms: 0,
                                                    active: false,
                                                });
                                                let _ = tx_for_timer.send(cancel_msg).await;
                                            }
                                        }
                                    } else {
                                        tracing::info!(symbol = %state.symbol, side = ?state.side, "Position already closed, canceling panic stop");
                                        let symbol_clone = state.symbol.clone();
                                        *state_guard = None;
                                        let cancel_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                                            symbol: symbol_clone,
                                            remaining_ms: 0,
                                            active: false,
                                        });
                                        let _ = tx_for_timer.send(cancel_msg).await;
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(symbol = %state.symbol, error = %e, "Failed to get positions for panic stop");
                                    let symbol_clone = state.symbol.clone();
                                    *state_guard = None;
                                    let cancel_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                                        symbol: symbol_clone,
                                        remaining_ms: 0,
                                        active: false,
                                    });
                                    let _ = tx_for_timer.send(cancel_msg).await;
                                }
                            }
                        }
                    } else if state.trigger_price.is_none() {
                        state.active = true;
                        state.last_trade_time = Instant::now();
                        tracing::info!(symbol = %state.symbol, side = ?state.side, timeout_ms = state.timeout_ms, "Panic stop activated immediately (no trigger price)");
                    }
                }
            }
        }
    }

    if let Some(handle) = chart_subscription.take() {
        handle.abort();
    }

    ws_event_task.abort();
    chart_event_task.abort();
    write_task.abort();

    tracing::info!("Client connection closed, all tasks cleaned up");
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
