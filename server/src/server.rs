use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use trade_shared::{ClientMessage, ClientPayload, ServerMessage, ServerPayload, Side, Symbol, Trade};

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
    let handler = ClientHandler::new(bybit);

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

    let panic_stop_state: Arc<Mutex<Option<PanicStopState>>> = Arc::new(Mutex::new(None));
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

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
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
            Ok(Message::Close(_)) => break,
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
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
