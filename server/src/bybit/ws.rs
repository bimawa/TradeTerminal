use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::sign::sign;
use crate::config::Config;

#[derive(Debug, Clone)]
#[allow(dead_code)]
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

#[allow(dead_code)]
pub struct BybitWebSocket {
    api_key: String,
    api_secret: String,
    private_url: String,
    public_url: String,
}

#[derive(Debug, Serialize)]
struct AuthMessage {
    op: String,
    args: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SubscribeMessage {
    op: String,
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WsMessage {
    topic: Option<String>,
    data: Option<serde_json::Value>,
    op: Option<String>,
    success: Option<bool>,
}

impl BybitWebSocket {
    pub fn new(config: &Config) -> Self {
        Self {
            api_key: config.bybit_api_key.clone(),
            api_secret: config.bybit_api_secret.clone(),
            private_url: config.ws_url().to_string(),
            public_url: config.ws_public_url().to_string(),
        }
    }

    pub async fn connect_private(&self, tx: mpsc::Sender<WsEvent>) -> Result<()> {
        const BASE_DELAY_SECS: u64 = 2;
        const MAX_DELAY_SECS: u64 = 30;
        let mut current_delay = BASE_DELAY_SECS;

        loop {
            match self.connect_private_once(&tx).await {
                Ok(_) => {
                    current_delay = BASE_DELAY_SECS;
                }
                Err(e) => {
                    tracing::error!("Bybit private WebSocket connection failed: {}", e);
                }
            }
            let _ = tx.send(WsEvent::Disconnected).await;
            tracing::info!("Reconnecting Bybit private WebSocket in {} seconds...", current_delay);
            tokio::time::sleep(std::time::Duration::from_secs(current_delay)).await;
            current_delay = (current_delay * 2).min(MAX_DELAY_SECS);
        }
    }

    async fn connect_private_once(&self, tx: &mpsc::Sender<WsEvent>) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.private_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64
            + 10000;
        let sign_str = format!("GET/realtime{}", expires);
        let signature = sign(&self.api_secret, &sign_str);

        let auth = AuthMessage {
            op: "auth".to_string(),
            args: vec![self.api_key.clone(), expires.to_string(), signature],
        };
        write.send(Message::Text(serde_json::to_string(&auth)?)).await?;

        let sub = SubscribeMessage {
            op: "subscribe".to_string(),
            args: vec![
                "order".to_string(),
                "position".to_string(),
                "execution".to_string(),
            ],
        };
        write.send(Message::Text(serde_json::to_string(&sub)?)).await?;

        let _ = tx.send(WsEvent::Connected).await;
        tracing::info!("Bybit private WebSocket connected");

        tokio::spawn(async move {
            let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(20));
            loop {
                ping_interval.tick().await;
                if write.send(Message::Text(r#"{"op":"ping"}"#.to_string())).await.is_err() {
                    break;
                }
            }
        });

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                        if let Some(topic) = ws_msg.topic {
                            if let Some(data) = ws_msg.data {
                                let event = match topic.as_str() {
                                    "order" => WsEvent::OrderUpdate(data),
                                    "position" => WsEvent::PositionUpdate(data),
                                    "execution" => WsEvent::ExecutionUpdate(data),
                                    _ => continue,
                                };
                                let _ = tx.send(event).await;
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    tracing::warn!("Bybit private WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    tracing::error!("Bybit private WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn connect_public(&self, symbols: Vec<String>, tx: mpsc::Sender<WsEvent>) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.public_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let topics: Vec<String> = symbols
            .iter()
            .map(|s| format!("tickers.{}", s))
            .collect();

        let sub = SubscribeMessage {
            op: "subscribe".to_string(),
            args: topics,
        };
        write.send(Message::Text(serde_json::to_string(&sub)?)).await?;

        tokio::spawn(async move {
            let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(20));
            loop {
                ping_interval.tick().await;
                if write.send(Message::Text(r#"{"op":"ping"}"#.to_string())).await.is_err() {
                    break;
                }
            }
        });

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                        if let Some(topic) = &ws_msg.topic {
                            if topic.starts_with("tickers.") {
                                if let Some(data) = ws_msg.data {
                                    let _ = tx.send(WsEvent::TickerUpdate(data)).await;
                                }
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(_) => break,
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn connect_chart(
        &self,
        symbol: &str,
        interval: &str,
        tx: mpsc::Sender<WsEvent>,
    ) -> Result<()> {
        const BASE_DELAY_SECS: u64 = 2;
        const MAX_DELAY_SECS: u64 = 30;
        let mut current_delay = BASE_DELAY_SECS;

        loop {
            match self.connect_chart_once(symbol, interval, &tx).await {
                Ok(_) => {
                    current_delay = BASE_DELAY_SECS;
                }
                Err(e) => {
                    tracing::error!("Bybit chart WebSocket connection failed for {}: {}", symbol, e);
                }
            }
            let _ = tx.send(WsEvent::Disconnected).await;
            tracing::info!(
                "Reconnecting Bybit chart WebSocket ({} {}) in {} seconds...",
                symbol,
                interval,
                current_delay
            );
            tokio::time::sleep(std::time::Duration::from_secs(current_delay)).await;
            current_delay = (current_delay * 2).min(MAX_DELAY_SECS);
        }
    }

    async fn connect_chart_once(
        &self,
        symbol: &str,
        interval: &str,
        tx: &mpsc::Sender<WsEvent>,
    ) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.public_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let topics = vec![
            format!("kline.{}.{}", interval, symbol),
            format!("publicTrade.{}", symbol),
        ];

        let sub = SubscribeMessage {
            op: "subscribe".to_string(),
            args: topics,
        };
        write
            .send(Message::Text(serde_json::to_string(&sub)?))
            .await?;

        let _ = tx.send(WsEvent::Connected).await;
        tracing::info!("Bybit chart WebSocket connected for {} {}", symbol, interval);

        tokio::spawn(async move {
            let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(20));
            loop {
                ping_interval.tick().await;
                if write
                    .send(Message::Text(r#"{"op":"ping"}"#.to_string()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                        if let Some(topic) = &ws_msg.topic {
                            if let Some(data) = ws_msg.data {
                                if topic.starts_with("kline.") {
                                    let _ = tx.send(WsEvent::KlineUpdate(data)).await;
                                } else if topic.starts_with("publicTrade.") {
                                    let _ = tx.send(WsEvent::TradeUpdate(data)).await;
                                }
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    tracing::warn!("Bybit chart WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    tracing::error!("Bybit chart WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }
}
