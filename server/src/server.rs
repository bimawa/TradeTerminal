use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use trade_shared::{ClientMessage, ServerMessage, ServerPayload};

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
                        if let Err(e) = handler.handle(client_msg, &tx).await {
                            tracing::error!("Handler error: {}", e);
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

    write_task.abort();
    Ok(())
}
