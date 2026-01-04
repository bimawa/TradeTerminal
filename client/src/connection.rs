use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use trade_shared::{AuthMessage, ClientMessage, ServerMessage, ServerPayload};
use url::Url;

use crate::tls;

pub struct Connection {
    url: String,
    rx_to_ui: mpsc::Sender<ServerMessage>,
    tx_to_server: mpsc::Sender<ClientMessage>,
    rx_from_ui: mpsc::Receiver<ClientMessage>,
    connected: bool,
}

impl Connection {
    pub fn new(url: &str, rx_to_ui: mpsc::Sender<ServerMessage>) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            url: url.to_string(),
            rx_to_ui,
            tx_to_server: tx,
            rx_from_ui: rx,
            connected: false,
        }
    }

    async fn send_disconnected(&self) {
        let msg = ServerMessage::new(ServerPayload::Disconnected);
        let _ = self.rx_to_ui.send(msg).await;
    }

    pub fn sender(&self) -> mpsc::Sender<ClientMessage> {
        self.tx_to_server.clone()
    }

    pub async fn run(mut self) -> Result<()> {
        const BASE_DELAY_SECS: u64 = 2;
        const MAX_DELAY_SECS: u64 = 30;
        let mut current_delay = BASE_DELAY_SECS;

        loop {
            match self.connect().await {
                Ok(_) => {
                    current_delay = BASE_DELAY_SECS;
                }
                Err(e) => {
                    tracing::error!("Connection failed: {}", e);
                    tracing::info!("Reconnecting in {} seconds...", current_delay);
                    tokio::time::sleep(std::time::Duration::from_secs(current_delay)).await;
                    current_delay = (current_delay * 2).min(MAX_DELAY_SECS);
                }
            }
        }
    }

    async fn connect(&mut self) -> Result<()> {
        if self.url.starts_with("wss://") {
            self.connect_tls().await
        } else {
            self.connect_plain().await
        }
    }

    async fn connect_plain(&mut self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();
        self.connected = true;

        loop {
            tokio::select! {
                Some(msg) = self.rx_from_ui.recv() => {
                    match serde_json::to_string(&msg) {
                        Ok(text) => {
                            if let Err(e) = write.send(Message::Text(text)).await {
                                tracing::error!("Failed to send message: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to serialize message: {}", e);
                        }
                    }
                }
                Some(msg) = read.next() => {
                    match msg {
                        Ok(Message::Text(text)) => {
                            match serde_json::from_str::<ServerMessage>(&text) {
                                Ok(server_msg) => {
                                    let _ = self.rx_to_ui.send(server_msg).await;
                                }
                                Err(e) => {
                                    tracing::error!("Failed to parse server message: {} - {}", e, &text[..text.len().min(200)]);
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            tracing::warn!("Server closed connection");
                            break;
                        }
                        Err(e) => {
                            tracing::error!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        if self.connected {
            self.connected = false;
            self.send_disconnected().await;
        }

        Ok(())
    }

    async fn connect_tls(&mut self) -> Result<()> {
        let expected_fingerprint = std::env::var("TLS_CERT_FINGERPRINT")
            .context("TLS_CERT_FINGERPRINT environment variable required for wss:// connections")?;

        let connector = tls::create_tls_connector(&expected_fingerprint)?;

        let url_obj = Url::parse(&self.url)?;
        let host = url_obj
            .host_str()
            .context("Invalid URL: missing host")?
            .to_string();
        let port = url_obj.port().unwrap_or(9000);

        let stream = tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await?;

        let domain = rustls::pki_types::ServerName::try_from(host.clone())
            .map_err(|_| anyhow::anyhow!("Invalid DNS name: {}", host))?
            .to_owned();

        let tls_stream = connector.connect(domain, stream).await?;

        let (ws_stream, _) = tokio_tungstenite::client_async(&self.url, tls_stream).await?;
        let (mut write, mut read) = ws_stream.split();

        let auth_key = std::env::var("AUTH_SECRET_KEY")
            .context("AUTH_SECRET_KEY environment variable required for wss:// connections")?;

        let auth_msg = AuthMessage {
            secret_key: auth_key,
        };
        let auth_json = serde_json::to_string(&auth_msg)?;

        write
            .send(Message::Text(auth_json))
            .await
            .context("Failed to send authentication")?;

        tracing::info!("Sent authentication, waiting for server response...");

        self.connected = true;

        loop {
            tokio::select! {
                Some(msg) = self.rx_from_ui.recv() => {
                    match serde_json::to_string(&msg) {
                        Ok(text) => {
                            if let Err(e) = write.send(Message::Text(text)).await {
                                tracing::error!("Failed to send message: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to serialize message: {}", e);
                        }
                    }
                }
                Some(msg) = read.next() => {
                    match msg {
                        Ok(Message::Text(text)) => {
                            match serde_json::from_str::<ServerMessage>(&text) {
                                Ok(server_msg) => {
                                    let _ = self.rx_to_ui.send(server_msg).await;
                                }
                                Err(e) => {
                                    tracing::error!("Failed to parse server message: {} - {}", e, &text[..text.len().min(200)]);
                                }
                            }
                        }
                        Ok(Message::Close(_)) => {
                            tracing::warn!("Server closed connection");
                            break;
                        }
                        Err(e) => {
                            tracing::error!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        if self.connected {
            self.connected = false;
            self.send_disconnected().await;
        }

        Ok(())
    }
}
