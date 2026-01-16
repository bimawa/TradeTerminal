use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

const MAX_UDP_PACKET_SIZE: usize = 1400;
const UDP_HEADER_SIZE: usize = 4 + 8 + 1 + 2;
const TCP_HEADER_SIZE: usize = 4 + 1 + 16;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Packet too large: {0} bytes (max {MAX_UDP_PACKET_SIZE})")]
    PacketTooLarge(usize),

    #[error("Packet too small: {0} bytes (min {UDP_HEADER_SIZE})")]
    PacketTooSmall(usize),

    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),

    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    PlaceOrder = 0,
    CancelOrder = 1,
    CancelAllOrders = 2,
    ClosePosition = 3,
    SetTrailingStop = 4,
    ActivityStop = 5,
    CancelActivityStop = 6,
    CreatePendingAutostop = 11,
    UpdatePendingAutostop = 12,
    CancelPendingAutostop = 13,
    GetPendingAutostops = 14,
    SubscribeChart = 7,
    UnsubscribeChart = 8,
    Subscribe = 9,
    Unsubscribe = 10,

    OrderPlaced = 20,
    OrderCancelled = 21,
    OrderError = 22,
    Error = 23,
    TrailingStopSet = 24,
    ActivityStopActivated = 25,
    PendingAutostopCreated = 29,
    PendingAutostopUpdated = 30,
    PendingAutostopCancelled = 31,
    PendingAutostopActivated = 32,
    PendingAutostops = 33,
    Connected = 26,
    ChartSubscribed = 27,
    PositionModeChanged = 28,

    GetPositions = 50,
    GetOrders = 51,
    GetTicker = 52,
    GetCandles = 53,
    GetAccountInfo = 54,
    Ping = 55,

    OrderUpdate = 70,
    PositionUpdate = 71,
    TradeUpdate = 72,
    TickerUpdate = 73,
    CandleUpdate = 74,
    Candles = 75,
    ActivityStopStatus = 76,
    ActivityStopTriggered = 77,
    Pong = 79,
    Positions = 80,
    Orders = 81,
    AccountInfo = 82,
    Disconnected = 83,
}

impl MessageType {
    pub fn is_tcp(&self) -> bool {
        (*self as u8) < 50
    }

    pub fn is_udp(&self) -> bool {
        (*self as u8) >= 50
    }

    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::PlaceOrder),
            1 => Ok(Self::CancelOrder),
            2 => Ok(Self::CancelAllOrders),
            3 => Ok(Self::ClosePosition),
            4 => Ok(Self::SetTrailingStop),
            5 => Ok(Self::ActivityStop),
            6 => Ok(Self::CancelActivityStop),
            11 => Ok(Self::CreatePendingAutostop),
            12 => Ok(Self::UpdatePendingAutostop),
            13 => Ok(Self::CancelPendingAutostop),
            14 => Ok(Self::GetPendingAutostops),
            7 => Ok(Self::SubscribeChart),
            8 => Ok(Self::UnsubscribeChart),
            9 => Ok(Self::Subscribe),
            10 => Ok(Self::Unsubscribe),

            20 => Ok(Self::OrderPlaced),
            21 => Ok(Self::OrderCancelled),
            22 => Ok(Self::OrderError),
            23 => Ok(Self::Error),
            24 => Ok(Self::TrailingStopSet),
            25 => Ok(Self::ActivityStopActivated),
            29 => Ok(Self::PendingAutostopCreated),
            30 => Ok(Self::PendingAutostopUpdated),
            31 => Ok(Self::PendingAutostopCancelled),
            32 => Ok(Self::PendingAutostopActivated),
            33 => Ok(Self::PendingAutostops),
            26 => Ok(Self::Connected),
            27 => Ok(Self::ChartSubscribed),
            28 => Ok(Self::PositionModeChanged),

            50 => Ok(Self::GetPositions),
            51 => Ok(Self::GetOrders),
            52 => Ok(Self::GetTicker),
            53 => Ok(Self::GetCandles),
            54 => Ok(Self::GetAccountInfo),
            55 => Ok(Self::Ping),

            70 => Ok(Self::OrderUpdate),
            71 => Ok(Self::PositionUpdate),
            72 => Ok(Self::TradeUpdate),
            73 => Ok(Self::TickerUpdate),
            74 => Ok(Self::CandleUpdate),
            75 => Ok(Self::Candles),
            76 => Ok(Self::ActivityStopStatus),
            77 => Ok(Self::ActivityStopTriggered),
            79 => Ok(Self::Pong),
            80 => Ok(Self::Positions),
            81 => Ok(Self::Orders),
            82 => Ok(Self::AccountInfo),
            83 => Ok(Self::Disconnected),

            _ => Err(ProtocolError::InvalidMessageType(value)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UdpPacket {
    pub sequence: u32,
    pub timestamp_us: u64,
    pub msg_type: MessageType,
    pub payload: Bytes,
}

#[derive(Debug, Clone)]
pub struct TcpFrame {
    pub msg_type: MessageType,
    pub request_id: Uuid,
    pub payload: Bytes,
}

fn timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}

pub fn encode_udp_packet(seq: u32, msg_type: MessageType, payload: &[u8]) -> Result<Bytes> {
    let total_len = UDP_HEADER_SIZE + payload.len();

    if total_len > MAX_UDP_PACKET_SIZE {
        return Err(ProtocolError::PacketTooLarge(total_len));
    }

    let mut buf = BytesMut::with_capacity(total_len);

    buf.put_u32_le(seq);
    buf.put_u64_le(timestamp_us());
    buf.put_u8(msg_type as u8);
    buf.put_u16_le(payload.len() as u16);
    buf.put(payload);

    Ok(buf.freeze())
}

pub fn encode_udp_packet_zero_copy(seq: u32, msg_type: MessageType, payload: Bytes) -> Result<Bytes> {
    let total_len = UDP_HEADER_SIZE + payload.len();

    if total_len > MAX_UDP_PACKET_SIZE {
        return Err(ProtocolError::PacketTooLarge(total_len));
    }

    let mut buf = BytesMut::with_capacity(total_len);

    buf.put_u32_le(seq);
    buf.put_u64_le(timestamp_us());
    buf.put_u8(msg_type as u8);
    buf.put_u16_le(payload.len() as u16);
    buf.put(payload);

    Ok(buf.freeze())
}

pub fn decode_udp_packet(data: &[u8]) -> Result<UdpPacket> {
    if data.len() < UDP_HEADER_SIZE {
        return Err(ProtocolError::PacketTooSmall(data.len()));
    }

    let mut buf = data;

    let sequence = buf.get_u32_le();
    let timestamp_us = buf.get_u64_le();
    let msg_type_byte = buf.get_u8();
    let payload_len = buf.get_u16_le() as usize;

    if buf.remaining() < payload_len {
        return Err(ProtocolError::PacketTooSmall(data.len()));
    }

    let msg_type = MessageType::from_u8(msg_type_byte)?;
    let payload = Bytes::copy_from_slice(&buf[..payload_len]);

    Ok(UdpPacket {
        sequence,
        timestamp_us,
        msg_type,
        payload,
    })
}

pub fn encode_tcp_frame(msg_type: MessageType, request_id: Uuid, payload: &[u8]) -> Result<Bytes> {
    let total_len = TCP_HEADER_SIZE + payload.len();
    let mut buf = BytesMut::with_capacity(total_len);

    buf.put_u32_le(total_len as u32 - 4);
    buf.put_u8(msg_type as u8);
    buf.put(request_id.as_bytes().as_slice());
    buf.put(payload);

    Ok(buf.freeze())
}

pub fn encode_tcp_frame_zero_copy(msg_type: MessageType, request_id: Uuid, payload: Bytes) -> Result<Bytes> {
    let total_len = TCP_HEADER_SIZE + payload.len();
    let mut buf = BytesMut::with_capacity(total_len);

    buf.put_u32_le(total_len as u32 - 4);
    buf.put_u8(msg_type as u8);
    buf.put(request_id.as_bytes().as_slice());
    buf.put(payload);

    Ok(buf.freeze())
}

pub fn decode_tcp_frame(data: &[u8]) -> Result<TcpFrame> {
    if data.len() < TCP_HEADER_SIZE - 4 {
        return Err(ProtocolError::PacketTooSmall(data.len()));
    }

    let mut buf = data;

    let msg_type_byte = buf.get_u8();
    let msg_type = MessageType::from_u8(msg_type_byte)?;

    let mut uuid_bytes = [0u8; 16];
    buf.copy_to_slice(&mut uuid_bytes);
    let request_id = Uuid::from_bytes(uuid_bytes);

    let payload = Bytes::copy_from_slice(buf);

    Ok(TcpFrame {
        msg_type,
        request_id,
        payload,
    })
}

pub fn batch_udp_messages(messages: Vec<(MessageType, Bytes)>) -> Result<Vec<Bytes>> {
    if messages.is_empty() {
        return Ok(Vec::new());
    }

    let mut packets = Vec::new();
    let mut current_batch = BytesMut::with_capacity(MAX_UDP_PACKET_SIZE);
    let mut batch_count = 0u16;

    for (msg_type, payload) in messages {
        let msg_size = 2 + payload.len();

        if current_batch.len() + msg_size > MAX_UDP_PACKET_SIZE - UDP_HEADER_SIZE
            && batch_count > 0 {
                packets.push(current_batch.freeze());
                current_batch = BytesMut::with_capacity(MAX_UDP_PACKET_SIZE);
                batch_count = 0;
            }

        current_batch.put_u8(msg_type as u8);
        current_batch.put_u16_le(payload.len() as u16);
        current_batch.put(payload);
        batch_count += 1;
    }

    if batch_count > 0 {
        packets.push(current_batch.freeze());
    }

    Ok(packets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_udp_packet_encode_decode() {
        let payload = b"test payload";
        let packet = encode_udp_packet(12345, MessageType::Pong, payload).unwrap();
        let decoded = decode_udp_packet(&packet).unwrap();

        assert_eq!(decoded.sequence, 12345);
        assert_eq!(decoded.msg_type, MessageType::Pong);
        assert_eq!(&decoded.payload[..], payload);
    }

    #[test]
    fn test_tcp_frame_roundtrip() {
        let request_id = Uuid::new_v4();
        let payload = b"test payload";
        let frame = encode_tcp_frame(MessageType::PlaceOrder, request_id, payload).unwrap();

        let decoded = decode_tcp_frame(&frame[4..]).unwrap();

        assert_eq!(decoded.request_id, request_id);
        assert_eq!(decoded.msg_type, MessageType::PlaceOrder);
        assert_eq!(&decoded.payload[..], payload);
    }

    #[test]
    fn test_message_type_classification() {
        assert!(MessageType::PlaceOrder.is_tcp());
        assert!(!MessageType::PlaceOrder.is_udp());
        assert!(MessageType::PositionUpdate.is_udp());
        assert!(!MessageType::PositionUpdate.is_tcp());
    }

    #[test]
    fn test_udp_packet_max_size() {
        let large_payload = vec![0u8; 2000];
        let result = encode_udp_packet(1, MessageType::Pong, &large_payload);
        assert!(result.is_err());
    }
}
