use std::io;

use crate::mc::proto::{Packet, PlayState};
use bytes::{Buf, BytesMut};
use log::{debug, trace};
use tokio_util::codec::Decoder;

pub trait MinecraftBufExt {
    fn has_complete_var_int(&mut self) -> bool;
    fn get_var_int(&mut self) -> i32;
    fn get_string(&mut self) -> String;
}

impl MinecraftBufExt for BytesMut {
    fn has_complete_var_int(&mut self) -> bool {
        for i in 0..std::cmp::min(4, self.len()) {
            let byte = self[i];
            if byte & 0x80 == 0 {
                return true;
            }
        }
        false
    }

    fn get_var_int(&mut self) -> i32 {
        let mut result = 0i32;
        for i in 0..4 {
            let byte = self.get_u8();
            let value = (byte & 0x7f) as i32;
            result |= value << i * 7;

            if byte & 0x80 == 0 {
                break;
            }
        }
        result
    }

    fn get_string(&mut self) -> String {
        let str_len = self.get_var_int();
        let str_data = self.split_to(str_len as usize);
        return String::from_utf8(str_data.to_vec()).expect("invalid string received");
    }
}

enum DecoderState {
    Header,
    Body(usize),
}

pub struct MinecraftCodec {
    compression_threshold: usize,
    current_state: PlayState,
    decoder_state: DecoderState,
}

impl MinecraftCodec {
    pub fn new() -> MinecraftCodec {
        MinecraftCodec {
            compression_threshold: 0,
            current_state: PlayState::Handshake,
            decoder_state: DecoderState::Header,
        }
    }

    pub fn change_state(&mut self, next_state: PlayState) {
        debug!("Changing to state {:?}", next_state);
        self.current_state = next_state;
    }

    fn decode_handshake_packet(&self, packet_id: i32, buf: &mut BytesMut) -> Option<Packet> {
        match packet_id {
            0x00 => Some(Packet::C00Handshake {
                protocol_version: buf.get_var_int(),
                server_address: buf.get_string(),
                server_port: buf.get_u16(),
                next_state: PlayState::try_from(buf.get_var_int())
                    .expect("Requested invalid state"),
            }),
            _ => None,
        }
    }

    fn decode_status_packet(&self, packet_id: i32, buf: &mut BytesMut) -> Option<Packet> {
        match packet_id {
            0x00 => Some(Packet::C00StatusRequest),
            0x01 => Some(Packet::C01StatusPing {
                timestamp: buf.get_i64(),
            }),
            _ => None,
        }
    }

    fn decode_login_packet(&self, packet_id: i32, buf: &mut BytesMut) -> Option<Packet> {
        match packet_id {
            0x00 => Some(Packet::C00LoginStart {
                username: buf.get_string(),
            }),
            _ => None,
        }
    }

    fn decode_play_packet(&self, packet_id: i32, buf: &mut BytesMut) -> Option<Packet> {
        match packet_id {
            0x00 => Some(Packet::C00KeepAlive {
                id: buf.get_var_int(),
            }),
            0x01 => Some(Packet::C01ChatMessage {
                message: buf.get_string(),
            }),
            _ => None,
        }
    }
}

impl Decoder for MinecraftCodec {
    type Item = Packet;

    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.decoder_state {
            DecoderState::Header => {
                if !src.has_complete_var_int() {
                    return Ok(None);
                }

                let packet_len = src.get_var_int() as usize;
                if packet_len > 1024 * 1024 * 1024 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Packet of length {} too large.", packet_len),
                    ));
                }

                self.decoder_state = DecoderState::Body(packet_len);
                self.decode(src)
            }
            DecoderState::Body(packet_len) => {
                if src.remaining() < packet_len {
                    return Ok(None);
                }
                self.decoder_state = DecoderState::Header;

                let mut payload = src.split_to(packet_len);
                if self.compression_threshold > 0 {
                    let _size_uncompressed = payload.get_var_int();
                    // TODO: Decompress here
                }

                let packet_id = payload.get_var_int();
                trace!("Decoding packet #{} with length {}", packet_id, packet_len);

                Ok(match self.current_state {
                    PlayState::Handshake => self.decode_handshake_packet(packet_id, &mut payload),
                    PlayState::Status => self.decode_status_packet(packet_id, &mut payload),
                    PlayState::Login => self.decode_login_packet(packet_id, &mut payload),
                    PlayState::Play => self.decode_play_packet(packet_id, &mut payload),
                })
            }
        }
    }
}
