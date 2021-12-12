use std::io;

use crate::mc::proto::{Packet, PlayState};
use bytes::{Buf, BufMut, BytesMut};
use log::trace;
use tokio_util::codec::{Decoder, Encoder};

pub trait MinecraftBufExt {
    fn has_complete_var_int(&mut self) -> bool;
    fn get_var_int(&mut self) -> i32;
    fn get_string(&mut self) -> String;
    fn get_bool(&mut self) -> bool;
    fn put_var_int(&mut self, value: i32);
    fn put_string(&mut self, value: &str);
    fn put_bool(&mut self, value: bool);
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

    fn get_bool(&mut self) -> bool {
        self.get_u8() != 0
    }

    fn put_var_int(&mut self, mut value: i32) {
        loop {
            let mut cur_byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                cur_byte |= 0x80;
            }
            self.put_u8(cur_byte);
            if value == 0 {
                break;
            }
        }
    }

    fn put_string(&mut self, value: &str) {
        self.put_var_int(value.len() as i32);
        self.extend_from_slice(value.as_bytes());
    }

    fn put_bool(&mut self, value: bool) {
        self.put_u8(if value { 1 } else { 0 });
    }
}

fn calc_varint_size(mut value: i32) -> usize {
    let mut size: usize = 0;
    loop {
        value >>= 7;
        size += 1;
        if value == 0 {
            break;
        }
    }
    size
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
        trace!("Changing to state {:?}", next_state);
        self.current_state = next_state;
    }

    pub fn change_compression_threshold(&mut self, compression_threshold: usize) {
        trace!(
            "Changing compression threshold to {}",
            compression_threshold
        );
        self.compression_threshold = compression_threshold;
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

    fn encode_packet(&self, packet: Packet, buf: &mut BytesMut) {
        match packet {
            Packet::S00StatusResponse { status } => buf.put_string(status.as_str()),
            Packet::S01StatusPong { timestamp } => buf.put_i64(timestamp),
            Packet::S02LoginSuccess { uuid, username } => {
                buf.put_string(uuid.as_str());
                buf.put_string(username.as_str());
            }
            Packet::S03LoginCompression { threshold } => buf.put_var_int(threshold),
            Packet::S00KeepAlive { timestamp } => buf.put_var_int(timestamp),
            Packet::S01JoinGame {
                entity_id,
                gamemode,
                dimension,
                difficulty,
                player_list_size,
                world_type,
                reduced_debug_info,
            } => {
                buf.put_i32(entity_id);
                buf.put_u8(gamemode);
                buf.put_u8(dimension);
                buf.put_u8(difficulty);
                buf.put_u8(player_list_size);
                buf.put_string(world_type.as_str());
                buf.put_bool(reduced_debug_info);
            }
            Packet::S08SetPlayerPosition {
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
            } => {
                buf.put_f64(x);
                buf.put_f64(y);
                buf.put_f64(z);
                buf.put_f32(yaw);
                buf.put_f32(pitch);
                buf.put_u8(flags);
            }
            _ => panic!("Invalid packet direction!"),
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

                src.reserve(packet_len);
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

impl Encoder<Packet> for MinecraftCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let packet_id: i32 = item.id();

        let mut packet_buf = BytesMut::new();
        self.encode_packet(item, &mut packet_buf);

        if self.compression_threshold > 0 {
            let packet_len = calc_varint_size(packet_id) + calc_varint_size(0) + packet_buf.len();
            dst.put_var_int(packet_len as i32);
            dst.put_var_int(0);
            // TODO: Compresssion
        } else {
            let packet_len = calc_varint_size(packet_id) + packet_buf.len();
            dst.put_var_int(packet_len as i32);
        }

        dst.put_var_int(packet_id);
        dst.extend_from_slice(&packet_buf[..]);

        Ok(())
    }
}
