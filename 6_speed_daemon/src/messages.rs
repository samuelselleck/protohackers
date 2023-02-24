use bytes::{ BytesMut, Buf, BufMut};
use std::io::Write;
use std::mem::size_of;
use std::time::Duration;
use std::{
    io::ErrorKind,
    time,
};
use tokio_util::codec;
pub type TimeStamp = u32;
pub type Mile = u16;
pub type Plate = String;
pub type RoadID = u16;
pub type Speed = u16;

#[derive(Debug, Clone)]
pub enum ClientMessage {
    PlateDetected {
        plate: Plate,
        time: TimeStamp,
    },
    HeartbeatRequest(time::Duration),
    IAmCamera {
        road: RoadID,
        position: Mile,
        speed_limit: Speed,
    },
    IAmDispatcher {
        roads: Vec<RoadID>,
    },
}

#[derive(Debug, Clone)]
pub enum ServerMessage {
    Error(String),
    Ticket {
        plate: Plate,
        road: RoadID,
        p1: (Mile, TimeStamp),
        p2: (Mile, TimeStamp),
    },
    HeartBeat,
}

pub struct MessageCodec;

impl MessageCodec {
    pub fn new() -> Self {
        MessageCodec {}
    }
}

impl codec::Decoder for MessageCodec {
    type Item = ClientMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let Some(message_type) = src.get(0) else { 
            src.reserve(1);
            return Ok(None)
        };

        let message = match message_type {
            0x20 => {
                let Some(len) = src.get(1) else {
                    src.reserve(2);
                    return Ok(None)
                };
                let len = *len as usize;
                let exp_len = 2 + len + size_of::<TimeStamp>();
                if src.len() <  exp_len {
                    src.reserve(exp_len);
                    return Ok(None);
                }

                src.advance(2);
                ClientMessage::PlateDetected {
                    plate: src.split_to(len).iter().map(|v| *v as char).collect(),
                    time: src.get_u32(),
                }
            },
            0x40 => {
                let exp_len = 1 + size_of::<TimeStamp>();
                if src.len() < exp_len {
                    src.reserve(exp_len);
                    return Ok(None)
                }
                src.advance(1);
                ClientMessage::HeartbeatRequest(
                    Duration::from_millis(src.get_u32() as u64 *100)
                )
            },
            0x80 => {
                let exp_len = 1 + size_of::<RoadID>() + size_of::<Mile>() + size_of::<Speed>();
                if src.len() < exp_len {
                    src.reserve(exp_len);
                    return Ok(None)
                };
                src.advance(1);
                ClientMessage::IAmCamera {
                    road: src.get_u16(),
                    position: src.get_u16(),
                    speed_limit: src.get_u16(),
                }
            },
            0x81 => {
                let Some(len) = src.get(1) else {
                    src.reserve(2);
                    return Ok(None)
                };
                let len = *len as usize;
                let exp_len = 2 + len*size_of::<RoadID>();
                if src.len() < exp_len {
                    src.reserve(exp_len);
                    return Ok(None)
                };
                src.advance(2);
                let mut arr = Vec::new();
                for _ in 0..len {
                    arr.push(src.get_u16());
                }
                
                ClientMessage::IAmDispatcher {
                    roads: arr,
                }
            },
            _ => {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    "invalid message type",
                ))
            }
        };
        Ok(Some(message))
    }
}

impl codec::Encoder<ServerMessage> for MessageCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: ServerMessage , dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            ServerMessage::Error(e) => {
                dst.put_u8(0x10);
                dst.put_u8(e.len() as u8);
                dst.writer().write(e.as_bytes()).unwrap();
            },
            ServerMessage::Ticket { plate, road, p1, p2 } => {
                dst.put_u8(0x21);
                dst.put_u8(plate.len() as u8);
                dst.writer().write(plate.as_bytes()).unwrap();
                dst.put_u16(road);
                let (mut mile1, mut time1) = p1;
                let (mut mile2, mut time2) = p2;
                if time2 < time1 {
                    std::mem::swap(&mut mile1, &mut mile2);
                    std::mem::swap(&mut time1, &mut time2);
                }
                dst.put_u16(mile1);
                dst.put_u32(time1);
                dst.put_u16(mile2);
                dst.put_u32(time2);
                let speed = (mile1 as f32 - mile2 as f32)/(time1 as f32 - time2 as f32).abs()*100.0;
                dst.put_u16(speed as u16);
            }
            ServerMessage::HeartBeat => {
                dst.put_u8(0x41);
            },
        }
        Ok(())
    }
}
