use anyhow::Result;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use trade_shared::{decode_udp_packet, encode_udp_packet, ClientMessage, ServerMessage};

struct SequenceTracker {
    last_sequence: u32,
    lost_packets: u64,
    out_of_order: u64,
    total_packets: u64,
}

impl SequenceTracker {
    fn new() -> Self {
        Self {
            last_sequence: 0,
            lost_packets: 0,
            out_of_order: 0,
            total_packets: 0,
        }
    }

    fn check_sequence(&mut self, seq: u32) {
        self.total_packets += 1;

        if self.total_packets == 1 {
            self.last_sequence = seq;
            return;
        }

        let expected = self.last_sequence.wrapping_add(1);

        if seq != expected {
            if seq > expected || (expected > u32::MAX - 1000 && seq < 1000) {
                let gap = seq.wrapping_sub(expected);
                self.lost_packets += gap as u64;
                tracing::warn!("UDP packet loss detected: {} packets dropped (seq: {} -> {})", gap, self.last_sequence, seq);
            } else {
                self.out_of_order += 1;
                tracing::debug!("Out-of-order UDP packet: seq={}, expected={}", seq, expected);
            }
        }

        self.last_sequence = seq;
    }

    fn log_stats(&self) {
        if self.total_packets > 0 {
            let loss_rate = (self.lost_packets as f64 / self.total_packets as f64) * 100.0;
            tracing::info!(
                "UDP stats: total={}, lost={} ({:.2}%), out_of_order={}",
                self.total_packets,
                self.lost_packets,
                loss_rate,
                self.out_of_order
            );
        }
    }
}

pub struct UdpClient {
    socket: UdpSocket,
    sequence_tracker: Arc<Mutex<SequenceTracker>>,
    rx_to_ui: mpsc::Sender<ServerMessage>,
}

impl UdpClient {
    pub async fn new(server_addr: &str, rx_to_ui: mpsc::Sender<ServerMessage>) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.set_recv_buffer_size(4 * 1024 * 1024)?;
        socket.connect(server_addr).await?;

        tracing::info!("UDP client connected to {}", server_addr);

        Ok(Self {
            socket,
            sequence_tracker: Arc::new(Mutex::new(SequenceTracker::new())),
            rx_to_ui,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        let tracker = self.sequence_tracker.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                let tracker = tracker.lock().await;
                tracker.log_stats();
            }
        });

        let mut buf = vec![0u8; 2048];
        loop {
            let (len, _) = self.socket.recv_from(&mut buf).await?;
            if let Err(e) = self.handle_packet(&buf[..len]).await {
                tracing::warn!("Failed to handle UDP packet: {}", e);
            }
        }
    }

    async fn handle_packet(&mut self, data: &[u8]) -> Result<()> {
        let packet = decode_udp_packet(data)?;

        {
            let mut tracker = self.sequence_tracker.lock().await;
            tracker.check_sequence(packet.sequence);
        }

        let msg: ServerMessage = bincode::deserialize(&packet.payload)?;
        self.rx_to_ui.send(msg).await?;

        Ok(())
    }

    pub async fn send_query(&self, msg: ClientMessage) -> Result<()> {
        let msg_type = msg.message_type();
        let payload = bincode::serialize(&msg)?;
        let packet = encode_udp_packet(0, msg_type, &payload)?;

        self.socket.send(&packet).await?;

        Ok(())
    }
}
