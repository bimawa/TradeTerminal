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
pub struct BinanceWebSocket {
    api_key: String,
    api_secret: String,
    private_url: String,
    public_url: String,
}

#[derive(Debug, Serialize)]
struct SubscribeMessage {
    method: String,
    params: Vec<String>,
    id: u64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WsMessage {
    #[serde(rename = "e")]
    event: Option<String>,
    #[serde(rename = "E")]
    event_time: Option<i64>,
    data: Option<serde_json::Value>,
    stream: Option<String>,
}

impl BinanceWebSocket {
    pub fn new(config: &Config) -> Self {
        Self {
            api_key: config.binance_api_key.clone(),
            api_secret: config.binance_api_secret.clone(),
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
                    tracing::error!("Binance private WebSocket connection failed: {}", e);
                }
            }
            let _ = tx.send(WsEvent::Disconnected).await;
            tracing::info!("Reconnecting Binance private WebSocket in {} seconds...", current_delay);
            tokio::time::sleep(std::time::Duration::from_secs(current_delay)).await;
            current_delay = (current_delay * 2).min(MAX_DELAY_SECS);
        }
    }

    async fn connect_private_once(&self, tx: &mpsc::Sender<WsEvent>) -> Result<()> {
        let listen_key = self.get_listen_key().await?;
        let ws_url = format!("{}/{}", self.private_url, listen_key);

        let (ws_stream, _) = connect_async(&ws_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let _ = tx.send(WsEvent::Connected).await;
        tracing::info!("Binance private WebSocket connected");

        let keepalive_tx = tx.clone();
        let api_key = self.api_key.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
            loop {
                interval.tick().await;
                if let Err(e) = Self::extend_listen_key(&api_key).await {
                    tracing::error!("Failed to extend listen key: {}", e);
                    let _ = keepalive_tx.send(WsEvent::Disconnected).await;
                    break;
                }
            }
        });

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    tracing::debug!("Binance WS received: {}", text);
                    if let Ok(ws_msg) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(event_type) = ws_msg.get("e").and_then(|e| e.as_str()) {
                            let event = match event_type {
                                "ORDER_TRADE_UPDATE" => WsEvent::OrderUpdate(ws_msg),
                                "ACCOUNT_UPDATE" => WsEvent::PositionUpdate(ws_msg),
                                _ => continue,
                            };
                            let _ = tx.send(event).await;
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("Binance WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    tracing::error!("Binance WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn connect_public(&self, tx: mpsc::Sender<WsEvent>, channels: Vec<String>) -> Result<()> {
        const BASE_DELAY_SECS: u64 = 2;
        const MAX_DELAY_SECS: u64 = 30;
        let mut current_delay = BASE_DELAY_SECS;

        loop {
            match self.connect_public_once(&tx, &channels).await {
                Ok(_) => {
                    current_delay = BASE_DELAY_SECS;
                }
                Err(e) => {
                    tracing::error!("Binance public WebSocket connection failed: {}", e);
                }
            }
            let _ = tx.send(WsEvent::Disconnected).await;
            tracing::info!("Reconnecting Binance public WebSocket in {} seconds...", current_delay);
            tokio::time::sleep(std::time::Duration::from_secs(current_delay)).await;
            current_delay = (current_delay * 2).min(MAX_DELAY_SECS);
        }
    }

    async fn connect_public_once(&self, tx: &mpsc::Sender<WsEvent>, channels: &[String]) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.public_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let subscribe = SubscribeMessage {
            method: "SUBSCRIBE".to_string(),
            params: channels.to_vec(),
            id: Self::timestamp(),
        };

        write.send(Message::Text(serde_json::to_string(&subscribe)?)).await?;

        let _ = tx.send(WsEvent::Connected).await;
        tracing::info!("Binance public WebSocket connected, subscribed to: {:?}", channels);

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    tracing::debug!("Binance public WS received: {}", text);
                    if let Ok(ws_msg) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(stream) = ws_msg.get("stream").and_then(|s| s.as_str()) {
                            let event = if stream.contains("@ticker") {
                                WsEvent::TickerUpdate(ws_msg)
                            } else if stream.contains("@kline") {
                                WsEvent::KlineUpdate(ws_msg)
                            } else if stream.contains("@trade") {
                                WsEvent::TradeUpdate(ws_msg)
                            } else {
                                continue;
                            };
                            let _ = tx.send(event).await;
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("Binance public WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    tracing::error!("Binance public WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn get_listen_key(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct ListenKeyResponse {
            #[serde(rename = "listenKey")]
            listen_key: String,
        }

        let client = reqwest::Client::new();
        let response = client
            .post("https://fapi.binance.com/fapi/v1/listenKey")
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await?;

        let resp: ListenKeyResponse = response.json().await?;
        Ok(resp.listen_key)
    }

    async fn extend_listen_key(api_key: &str) -> Result<()> {
        let client = reqwest::Client::new();
        let _ = client
            .put("https://fapi.binance.com/fapi/v1/listenKey")
            .header("X-MBX-APIKEY", api_key)
            .send()
            .await?;
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
                    tracing::error!("Binance chart WebSocket connection failed for {}: {}", symbol, e);
                }
            }
            let _ = tx.send(WsEvent::Disconnected).await;
            tracing::info!(
                "Reconnecting Binance chart WebSocket ({} {}) in {} seconds...",
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
        let channel = format!("{}@kline_{}", symbol.to_lowercase(), interval);
        let (ws_stream, _) = connect_async(&self.public_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let subscribe = SubscribeMessage {
            method: "SUBSCRIBE".to_string(),
            params: vec![channel.clone()],
            id: Self::timestamp(),
        };

        write.send(Message::Text(serde_json::to_string(&subscribe)?)).await?;

        let _ = tx.send(WsEvent::Connected).await;
        tracing::info!("Binance chart WebSocket connected: {}", channel);

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(ws_msg) = serde_json::from_str::<serde_json::Value>(&text) {
                        if ws_msg.get("e").and_then(|e| e.as_str()) == Some("kline") {
                            let _ = tx.send(WsEvent::KlineUpdate(ws_msg)).await;
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("Binance chart WebSocket closed");
                    break;
                }
                Err(e) => {
                    tracing::error!("Binance chart WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    }
}
