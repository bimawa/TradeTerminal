use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use trade_shared::{ClientMessage, ServerMessage};

pub struct Connection {
    url: String,
    rx_to_ui: mpsc::Sender<ServerMessage>,
    tx_to_server: mpsc::Sender<ClientMessage>,
    rx_from_ui: mpsc::Receiver<ClientMessage>,
}

impl Connection {
    pub fn new(url: &str, rx_to_ui: mpsc::Sender<ServerMessage>) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            url: url.to_string(),
            rx_to_ui,
            tx_to_server: tx,
            rx_from_ui: rx,
        }
    }

    pub fn sender(&self) -> mpsc::Sender<ClientMessage> {
        self.tx_to_server.clone()
    }

    pub async fn run(mut self) -> Result<()> {
        loop {
            match self.connect().await {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("Connection failed: {}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    }

    async fn connect(&mut self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();

        loop {
            tokio::select! {
                Some(msg) = self.rx_from_ui.recv() => {
                    let text = serde_json::to_string(&msg)?;
                    write.send(Message::Text(text)).await?;
                }
                Some(msg) = read.next() => {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                                let _ = self.rx_to_ui.send(server_msg).await;
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
            }
        }

        Ok(())
    }
}
