use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use trade_shared::{AuthMessage, ClientMessage, ClientPayload, OrderRequest, OrderType, PersistedPanicStopState, ServerMessage, ServerPayload, Side, Symbol, TimeInForce, Trade};

use crate::{auth, tls};

pub struct PanicStopState {
    pub persisted: PersistedPanicStopState,
    pub last_trade_time: Instant,
}

use crate::bybit::{BybitClient, BybitWebSocket, WsEvent};
use crate::client_handler::ClientHandler;
use crate::config::Config;
use crate::db;

pub async fn run(config: Config, db: Arc<redb::Database>) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr)
        .await
        .context(format!("Failed to bind to {}", config.listen_addr))?;
    let bybit_client = Arc::new(BybitClient::new(&config));
    let bybit_ws = Arc::new(BybitWebSocket::new(&config));

    if let Err(e) = bybit_client.switch_to_hedge_mode().await {
        tracing::warn!(error = %e, "Failed to switch to hedge mode (may already be in hedge mode)");
    }

    let tls_acceptor = if config.tls_enabled {
        let tls_config = tls::load_tls_config(
            config.tls_cert_path.as_ref().context("TLS_CERT_PATH required when TLS_ENABLED=true")?,
            config.tls_key_path.as_ref().context("TLS_KEY_PATH required when TLS_ENABLED=true")?,
        )?;
        Some(TlsAcceptor::from(Arc::new(tls_config)))
    } else {
        None
    };

    let auth_key = config.auth_secret_key.clone();

    tracing::info!("Server listening on {} (TLS: {})", config.listen_addr, config.tls_enabled);

    while let Ok((stream, addr)) = listener.accept().await {
        tracing::info!("New connection from {}", addr);
        let bybit = bybit_client.clone();
        let ws = bybit_ws.clone();
        let db_clone = db.clone();
        let acceptor = tls_acceptor.clone();
        let auth = auth_key.clone();

        tokio::spawn(async move {
            let result = if let Some(tls_acceptor) = acceptor {
                match tls_acceptor.accept(stream).await {
                    Ok(tls_stream) => handle_connection(tls_stream, bybit, ws, db_clone, auth).await,
                    Err(e) => {
                        tracing::error!(addr = %addr, error = %e, "TLS handshake failed");
                        return;
                    }
                }
            } else {
                handle_connection(stream, bybit, ws, db_clone, auth).await
            };

            if let Err(e) = result {
                tracing::error!(addr = %addr, error = %e, "Connection error");
            }
        });
    }

    Ok(())
}

async fn handle_connection(
    stream: impl AsyncRead + AsyncWrite + Unpin + Send + 'static,
    bybit: Arc<BybitClient>,
    bybit_ws: Arc<BybitWebSocket>,
    db: Arc<redb::Database>,
    auth_key: Option<String>,
) -> Result<()> {
    let ws_stream = accept_async(stream)
        .await
        .context("Failed to accept WebSocket connection")?;
    let (mut write, mut read) = ws_stream.split();

    if let Some(expected_key) = auth_key {
        match tokio::time::timeout(auth::AUTH_TIMEOUT, read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                match serde_json::from_str::<AuthMessage>(&text) {
                    Ok(auth_msg) => {
                        if !auth::validate_auth_key(&auth_msg.secret_key, &expected_key) {
                            tracing::warn!("Authentication failed: invalid key");
                            let error_msg = serde_json::to_string(&ServerMessage::new(
                                ServerPayload::Error {
                                    code: 401,
                                    message: "Invalid authentication key".into(),
                                }
                            ))?;
                            write.send(Message::Text(error_msg)).await?;
                            anyhow::bail!("Authentication failed");
                        }
                        tracing::info!("Client authenticated successfully");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to parse auth message");
                        anyhow::bail!("Invalid authentication message");
                    }
                }
            }
            Ok(Some(Ok(_))) => anyhow::bail!("Expected text message for authentication"),
            Ok(Some(Err(e))) => anyhow::bail!("WebSocket error during auth: {}", e),
            Ok(None) => anyhow::bail!("Connection closed during authentication"),
            Err(_) => anyhow::bail!("Authentication timeout"),
        }
    }

    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);
    let bybit_for_timer = bybit.clone();

    let initial_state = match db::load_all_panic_stops(&db) {
        Ok(states) => {
            if let Some(persisted) = states.into_iter().next() {
                let current_timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);

                let elapsed_ms = (current_timestamp - persisted.start_timestamp).max(0) as u64;

                if persisted.active && elapsed_ms >= persisted.timeout_ms {
                    tracing::warn!(
                        symbol = %persisted.symbol,
                        side = ?persisted.side,
                        "Panic stop timer expired during restart, will trigger on next tick"
                    );
                }

                let last_trade_time = Instant::now() - Duration::from_millis(elapsed_ms.min(persisted.timeout_ms));

                tracing::info!(
                    symbol = %persisted.symbol,
                    side = ?persisted.side,
                    active = persisted.active,
                    elapsed_ms = elapsed_ms,
                    remaining_ms = persisted.timeout_ms.saturating_sub(elapsed_ms),
                    "Restored panic stop state from database"
                );

                Some(PanicStopState {
                    persisted,
                    last_trade_time,
                })
            } else {
                None
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load panic stop states from database");
            None
        }
    };

    let panic_stop_state: Arc<Mutex<Option<PanicStopState>>> = Arc::new(Mutex::new(initial_state));
    let handler = ClientHandler::new(bybit, panic_stop_state.clone(), db.clone());

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
    let db_for_ws = db.clone();
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
                                let matching_position = positions.iter().find(|p| {
                                    p.symbol == state.persisted.symbol && p.side == state.persisted.side
                                });

                                let should_cancel = match matching_position {
                                    None => true,
                                    Some(pos) => pos.quantity == rust_decimal::Decimal::ZERO,
                                };

                                if should_cancel {
                                    tracing::info!(
                                        symbol = %state.persisted.symbol,
                                        side = ?state.persisted.side,
                                        "Panic stop canceled: position closed or zeroed"
                                    );
                                    let symbol_clone = state.persisted.symbol.clone();
                                    let side_for_delete = state.persisted.side;
                                    if let Err(e) = db::delete_panic_stop(&db_for_ws, &symbol_clone.0, side_for_delete) {
                                        tracing::warn!(error = %e, "Failed to delete panic stop from database");
                                    }
                                    *state_guard = None;
                                    let cancel_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                                        symbol: symbol_clone,
                                        remaining_ms: 0,
                                        active: false,
                                        trigger_price: None,
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
                            if subscribed_symbol.as_ref() == Some(&state.persisted.symbol) {
                                if let Some(trigger) = state.persisted.trigger_price {
                                    let triggered = match state.persisted.side {
                                        Side::Buy => trade_price >= trigger,
                                        Side::Sell => trade_price <= trigger,
                                    };
                                    if triggered && !state.persisted.active {
                                        state.persisted.active = true;
                                        state.last_trade_time = Instant::now();
                                        tracing::info!(symbol = %state.persisted.symbol, trigger_price = %trigger, trade_price = %trade_price, side = ?state.persisted.side, "Panic stop activated at trigger price");
                                    }
                                }
                                if state.persisted.active {
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
                    if state.persisted.active {
                        let elapsed = state.last_trade_time.elapsed().as_millis() as u64;
                        let remaining_ms = state.persisted.timeout_ms.saturating_sub(elapsed);

                        let status_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                            symbol: state.persisted.symbol.clone(),
                            remaining_ms,
                            active: true,
                            trigger_price: state.persisted.trigger_price,
                        });
                        let _ = tx_for_timer.send(status_msg).await;

                        if elapsed >= state.persisted.timeout_ms {
                            tracing::info!(symbol = %state.persisted.symbol, side = ?state.persisted.side, timeout_ms = state.persisted.timeout_ms, "Panic stop timeout reached, executing market close");



                            match bybit_for_timer.get_positions(Some(&state.persisted.symbol)).await {
                                Ok(positions) => {
                                    tracing::debug!(symbol = %state.persisted.symbol, side = ?state.persisted.side, positions_count = positions.len(), "Fetched positions for panic stop");
                                    for pos in &positions {
                                        tracing::debug!(pos_symbol = %pos.symbol, pos_side = ?pos.side, pos_qty = %pos.quantity, "Position details");
                                    }
                                    if let Some(position) = positions.iter().find(|p| p.symbol == state.persisted.symbol && p.quantity > rust_decimal::Decimal::ZERO) {
                                        let actual_close_side = match position.side {
                                            Side::Buy => Side::Sell,
                                            Side::Sell => Side::Buy,
                                        };
                                        let position_idx = match position.side {
                                            Side::Buy => 1,
                                            Side::Sell => 2,
                                        };
                                        tracing::info!(symbol = %state.persisted.symbol, original_side = ?state.persisted.side, actual_side = ?position.side, position_idx = position_idx, qty = %position.quantity, "Found position, attempting to close (side may have changed)");
                                        let order_req = OrderRequest {
                                            symbol: state.persisted.symbol.clone(),
                                            side: actual_close_side,
                                            order_type: OrderType::Market,
                                            quantity: position.quantity,
                                            price: None,
                                            time_in_force: TimeInForce::Ioc,
                                            reduce_only: true,
                                            take_profit: None,
                                            stop_loss: None,
                                            position_idx: Some(position_idx),
                                        };

                                        match bybit_for_timer.place_order(&order_req).await {
                                            Ok(_) => {
                                                tracing::info!(symbol = %state.persisted.symbol, side = ?state.persisted.side, qty = %position.quantity, "Panic stop market close executed");
                                                let symbol_clone = state.persisted.symbol.clone();
                                                let side_for_delete = state.persisted.side;
                                                if let Err(e) = db::delete_panic_stop(&db, &symbol_clone.0, side_for_delete) {
                                                    tracing::warn!(error = %e, "Failed to delete panic stop from database");
                                                }
                                                *state_guard = None;
                                                let triggered_msg = ServerMessage::new(ServerPayload::PanicStopTriggered {
                                                    symbol: symbol_clone,
                                                });
                                                let _ = tx_for_timer.send(triggered_msg).await;
                                            }
                                            Err(e) => {
                                                let symbol_clone = state.persisted.symbol.clone();
                                                let pos_side = position.side;
                                                let side_for_delete = state.persisted.side;
                                                let error_string = e.to_string();
                                                tracing::error!(symbol = %symbol_clone, actual_side = ?pos_side, error = %error_string, "Failed to execute panic stop market close");
                                                if let Err(e) = db::delete_panic_stop(&db, &symbol_clone.0, side_for_delete) {
                                                    tracing::warn!(error = %e, "Failed to delete panic stop from database");
                                                }
                                                *state_guard = None;
                                                let error_msg = ServerMessage::new(ServerPayload::OrderError {
                                                    message: format!("Failed to close position for {} {:?}: {}", symbol_clone, pos_side, error_string),
                                                });
                                                let _ = tx_for_timer.send(error_msg).await;
                                            }
                                        }
                                    } else {
                                        tracing::info!(symbol = %state.persisted.symbol, side = ?state.persisted.side, "Position already closed, canceling panic stop");
                                        let symbol_clone = state.persisted.symbol.clone();
                                        let side_for_delete = state.persisted.side;
                                        if let Err(e) = db::delete_panic_stop(&db, &symbol_clone.0, side_for_delete) {
                                            tracing::warn!(error = %e, "Failed to delete panic stop from database");
                                        }
                                        *state_guard = None;
                                        let cancel_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                                            symbol: symbol_clone,
                                            remaining_ms: 0,
                                            active: false,
                                            trigger_price: None,
                                        });
                                        let _ = tx_for_timer.send(cancel_msg).await;
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(symbol = %state.persisted.symbol, error = %e, "Failed to get positions for panic stop");
                                    let symbol_clone = state.persisted.symbol.clone();
                                    let side_for_delete = state.persisted.side;
                                    if let Err(del_e) = db::delete_panic_stop(&db, &symbol_clone.0, side_for_delete) {
                                        tracing::warn!(error = %del_e, "Failed to delete panic stop from database");
                                    }
                                    *state_guard = None;
                                    let cancel_msg = ServerMessage::new(ServerPayload::PanicStopStatus {
                                        symbol: symbol_clone,
                                        remaining_ms: 0,
                                        active: false,
                                        trigger_price: None,
                                    });
                                    let _ = tx_for_timer.send(cancel_msg).await;
                                }
                            }
                        }
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
