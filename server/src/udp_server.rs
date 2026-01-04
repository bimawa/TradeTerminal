use anyhow::Result;
use bytes::Bytes;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use trade_shared::{encode_udp_packet, MessageType, ServerMessage, ServerPayload};

pub struct UdpServer {
    socket: Arc<UdpSocket>,
    sequence: Arc<AtomicU32>,
    client_addr: Arc<RwLock<Option<SocketAddr>>>,
}

impl UdpServer {
    pub async fn new(bind_addr: &str) -> Result<Arc<Self>> {
        let socket = UdpSocket::bind(bind_addr).await?;

        socket.set_recv_buffer_size(4 * 1024 * 1024)?;
        socket.set_send_buffer_size(4 * 1024 * 1024)?;

        tracing::info!("UDP server bound to {}", bind_addr);

        Ok(Arc::new(Self {
            socket: Arc::new(socket),
            sequence: Arc::new(AtomicU32::new(0)),
            client_addr: Arc::new(RwLock::new(None)),
        }))
    }

    pub async fn send_message(&self, msg: ServerMessage) -> Result<()> {
        let client_addr = self.client_addr.read().await;
        if let Some(addr) = *client_addr {
            let msg_type = msg.message_type();
            let payload = bincode::serialize(&msg)?;
            let packet = encode_udp_packet(self.next_sequence(), msg_type, &payload)?;

            self.socket.send_to(&packet, addr).await?;
        }

        Ok(())
    }

    pub async fn send_batch(&self, messages: Vec<ServerMessage>) -> Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        let client_addr = self.client_addr.read().await;
        if let Some(addr) = *client_addr {
            for msg in messages {
                let msg_type = msg.message_type();
                let payload = bincode::serialize(&msg)?;
                let packet = encode_udp_packet(self.next_sequence(), msg_type, &payload)?;

                self.socket.send_to(&packet, addr).await?;
            }
        }

        Ok(())
    }

    pub async fn recv_loop(
        self: Arc<Self>,
        handler_tx: tokio::sync::mpsc::Sender<(ServerMessage, Option<SocketAddr>)>,
    ) {
        let mut buf = vec![0u8; 2048];

        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    {
                        let mut client_addr = self.client_addr.write().await;
                        if client_addr.is_none() {
                            *client_addr = Some(addr);
                            tracing::info!("UDP client connected from {}", addr);
                        }
                    }

                    match self.handle_packet(&buf[..len], addr).await {
                        Ok(Some(msg)) => {
                            if let Err(e) = handler_tx.send((msg, Some(addr))).await {
                                tracing::error!("Failed to send message to handler: {}", e);
                                break;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::warn!("Failed to handle UDP packet: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("UDP recv error: {}", e);
                    break;
                }
            }
        }
    }

    async fn handle_packet(
        &self,
        data: &[u8],
        _addr: SocketAddr,
    ) -> Result<Option<ServerMessage>> {
        let packet = trade_shared::decode_udp_packet(data)?;

        let msg: ServerMessage = bincode::deserialize(&packet.payload)?;

        Ok(Some(msg))
    }

    fn next_sequence(&self) -> u32 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }

    pub async fn set_client_addr(&self, addr: SocketAddr) {
        let mut client_addr = self.client_addr.write().await;
        *client_addr = Some(addr);
        tracing::info!("UDP client address set to {}", addr);
    }
}
