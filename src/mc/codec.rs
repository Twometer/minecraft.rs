use std::{f32::consts::PI, io};

use bytes::{Buf, BufMut, BytesMut};
use log::{debug, trace};
use tokio_util::codec::{Decoder, Encoder};

use crate::{
    mc::{
        proto::{DiggingStatus, EntityMetaData, Packet, PlayState, PlayerListItemAction},
        zlib,
    },
    model::ItemStack,
    world::{BlockFace, BlockPos},
};

const PACKET_SIZE_LIMIT: usize = 2 * 1024 * 1024;

pub trait MinecraftBufExt {
    fn has_complete_var_int(&mut self) -> bool;
    fn get_var_int(&mut self) -> i32;
    fn get_string(&mut self) -> String;
    fn get_bool(&mut self) -> bool;
    fn put_var_int(&mut self, value: i32);
    fn put_string(&mut self, value: &str);
    fn put_bool(&mut self, value: bool);
    fn put_angle(&mut self, value: f32);
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

    fn put_angle(&mut self, value: f32) {
        let scaled = value / (2.0 * PI) * 255.0;
        self.put_u8(scaled as u8);
    }
}

fn calc_var_int_size(mut value: i32) -> usize {
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
    play_state: PlayState,
    decoder_state: DecoderState,
}

impl MinecraftCodec {
    pub fn new() -> MinecraftCodec {
        MinecraftCodec {
            compression_threshold: 0,
            play_state: PlayState::Handshake,
            decoder_state: DecoderState::Header,
        }
    }

    pub fn set_state(&mut self, next_state: PlayState) {
        debug!("Changing to state {:?}", next_state);
        self.play_state = next_state;
    }

    pub fn set_compression_threshold(&mut self, compression_threshold: usize) {
        debug!(
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
            0x03 => Some(Packet::C03Player {
                on_ground: buf.get_bool(),
            }),
            0x04 => Some(Packet::C04PlayerPos {
                x: buf.get_f64(),
                y: buf.get_f64(),
                z: buf.get_f64(),
                on_ground: buf.get_bool(),
            }),
            0x05 => Some(Packet::C05PlayerRot {
                yaw: buf.get_f32(),
                pitch: buf.get_f32(),
                on_ground: buf.get_bool(),
            }),
            0x06 => Some(Packet::C06PlayerPosRot {
                x: buf.get_f64(),
                y: buf.get_f64(),
                z: buf.get_f64(),
                yaw: buf.get_f32(),
                pitch: buf.get_f32(),
                on_ground: buf.get_bool(),
            }),
            0x07 => Some(Packet::C07PlayerDigging {
                status: DiggingStatus::from(buf.get_u8()),
                location: BlockPos::from(buf.get_u64()),
                face: buf.get_u8(),
            }),
            0x08 => Some(Packet::C08PlayerBlockPlacement {
                location: BlockPos::from(buf.get_u64()),
                face: BlockFace::from(buf.get_u8()),
            }),
            0x09 => Some(Packet::C09HeldItemChange {
                slot: buf.get_i16(),
            }),
            0x0A => Some(Packet::C0AAnimation),
            0x10 => Some(Packet::C10SetCreativeSlot {
                slot_id: buf.get_i16(),
                item: ItemStack::read(buf),
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
                game_mode,
                dimension,
                difficulty,
                player_list_size,
                world_type,
                reduced_debug_info,
            } => {
                buf.put_i32(entity_id);
                buf.put_u8(game_mode as u8);
                buf.put_u8(dimension);
                buf.put_u8(difficulty);
                buf.put_u8(player_list_size);
                buf.put_string(world_type.as_str());
                buf.put_bool(reduced_debug_info);
            }
            Packet::S02ChatMessage {
                json_data,
                position,
            } => {
                buf.put_string(&json_data);
                buf.put_u8(position);
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
            Packet::S21ChunkData { x, z } => {
                buf.put_i32(x);
                buf.put_i32(z);
                buf.put_bool(true);
                buf.put_u16(0);
                buf.put_var_int(0);
            }
            Packet::S23BlockChange {
                location,
                block_state,
            } => {
                buf.put_u64(location.to_u64());
                buf.put_var_int(block_state as i32);
            }
            Packet::S26MapChunkBulk { skylight, chunks } => {
                buf.put_bool(skylight);
                buf.put_var_int(chunks.len() as i32);

                // Estimate size of final chunk data array to reduce reallocations
                let avg_section_size = 4096 + 2 * 4096;
                let estimated_chunk_array_len = chunks.len() * (256 + 4 * avg_section_size);
                let mut chunk_buf = BytesMut::with_capacity(estimated_chunk_array_len);

                for chunk in chunks {
                    let mut bitmask: u16 = 0;
                    let mut num_sections = 0;

                    // Write blocks and bitmask to data buffer
                    for i in 0..chunk.sections.len() {
                        let section = &chunk.sections[i];
                        if section.is_some() {
                            bitmask |= 1 << i;
                            num_sections += 1;

                            let section = section.as_ref().unwrap();
                            for block_state in section.data {
                                chunk_buf.put_u16_le(block_state);
                            }
                        }
                    }

                    // Write dummy lighting (Max value everywhere) to data buffer
                    for _ in 0..(4096 * num_sections / 8) {
                        chunk_buf.put_u64(0xffffffffffffffff);
                    }

                    // Write biomes to data buffer
                    chunk_buf.extend_from_slice(&chunk.biomes[..]);

                    // Write metadata to main buffer
                    buf.put_i32(chunk.x);
                    buf.put_i32(chunk.z);
                    buf.put_u16(bitmask);
                }

                // Copy data buffer to main buffer
                buf.extend_from_slice(&chunk_buf[..]);
            }
            Packet::S0ESpawnObject {
                entity_id,
                kind,
                x,
                y,
                z,
                pitch,
                yaw,
                data,
            } => {
                buf.put_var_int(entity_id);
                buf.put_u8(kind);
                buf.put_i32((x * 32.0) as i32);
                buf.put_i32((y * 32.0) as i32);
                buf.put_i32((z * 32.0) as i32);
                buf.put_angle(pitch);
                buf.put_angle(yaw);
                buf.put_i32(data);
            }
            Packet::S1CEntityMeta { entity_id, entries } => {
                if entries.is_empty() {
                    panic!("At least one entity meta entry is required!");
                }

                buf.put_var_int(entity_id);
                for entry in entries {
                    buf.put_u8(entry.data.type_id() << 5 | entry.index & 0x1f);
                    match entry.data {
                        EntityMetaData::Byte(v) => buf.put_u8(v),
                        EntityMetaData::Short(v) => buf.put_i16(v),
                        EntityMetaData::Int(v) => buf.put_i32(v),
                        EntityMetaData::Float(v) => buf.put_f32(v),
                        EntityMetaData::String(v) => buf.put_string(v.as_str()),
                        EntityMetaData::Slot(itm) => {
                            buf.put_i16(itm.id);
                            buf.put_u8(itm.count);
                            buf.put_u16(itm.damage);
                            buf.put_u8(0);
                        }
                        EntityMetaData::Vec3i { x, y, z } => {
                            buf.put_i32(x);
                            buf.put_i32(y);
                            buf.put_i32(z);
                        }
                        EntityMetaData::Vec3f { x, y, z } => {
                            buf.put_f32(x);
                            buf.put_f32(y);
                            buf.put_f32(z);
                        }
                    }
                }
                buf.put_u8(0x7f);
            }
            Packet::S2BChangeGameState { reason, value } => {
                buf.put_u8(reason as u8);
                buf.put_f32(value);
            }
            Packet::S38PlayerListItem { uuid, action } => {
                buf.put_var_int(action.id());
                buf.put_var_int(1);
                buf.put_u128(uuid.as_u128());
                match action {
                    PlayerListItemAction::AddPlayer {
                        name,
                        game_mode,
                        ping,
                        display_name,
                    } => {
                        buf.put_string(name.as_str());
                        buf.put_var_int(0);
                        buf.put_var_int(game_mode as i32);
                        buf.put_var_int(ping);
                        buf.put_bool(display_name.is_some());
                        if display_name.is_some() {
                            buf.put_string(display_name.unwrap().as_str());
                        }
                    }
                    PlayerListItemAction::UpdateGameMode { game_mode } => {
                        buf.put_var_int(game_mode as i32);
                    }
                    PlayerListItemAction::UpdateLatency { ping } => {
                        buf.put_var_int(ping);
                    }
                    PlayerListItemAction::UpdateDisplayName { display_name } => {
                        buf.put_bool(display_name.is_some());
                        if display_name.is_some() {
                            buf.put_string(display_name.unwrap().as_str());
                        }
                    }
                    PlayerListItemAction::RemovePlayer { .. } => {}
                }
            }
            Packet::S39PlayerAbilities {
                flags,
                flying_speed,
                walking_speed,
            } => {
                let mut flags_byte = 0u8;
                if flags.god_mode {
                    flags_byte |= 1;
                }
                if flags.is_flying {
                    flags_byte |= 2;
                }
                if flags.allow_flying {
                    flags_byte |= 4;
                }
                if flags.is_creative {
                    flags_byte |= 8;
                }
                buf.put_u8(flags_byte);
                buf.put_f32(flying_speed);
                buf.put_f32(walking_speed);
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
                if packet_len > PACKET_SIZE_LIMIT {
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
                    let size_uncompressed = payload.get_var_int();
                    if size_uncompressed > 0 {
                        payload = zlib::decompress(&payload[..]);
                    }
                }

                let packet_id = payload.get_var_int();
                trace!("Decoding packet #{} with length {}", packet_id, packet_len);

                Ok(match self.play_state {
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
        packet_buf.put_var_int(packet_id);
        self.encode_packet(item, &mut packet_buf);

        if packet_buf.len() > PACKET_SIZE_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Packet of length {} too large.", packet_buf.len()),
            ));
        }

        if self.compression_threshold > 0 {
            if packet_buf.len() > self.compression_threshold {
                let packet_buf_compressed = zlib::compress(&packet_buf[..]);

                let data_len = packet_buf.len() as i32;
                let packet_len = (calc_var_int_size(data_len) + packet_buf_compressed.len()) as i32;

                dst.put_var_int(packet_len);
                dst.put_var_int(data_len);
                dst.extend_from_slice(&packet_buf_compressed[..]);
            } else {
                dst.put_var_int(packet_buf.len() as i32 + 1);
                dst.put_var_int(0);
                dst.extend_from_slice(&packet_buf[..]);
            }
        } else {
            dst.put_var_int(packet_buf.len() as i32);
            dst.extend_from_slice(&packet_buf[..]);
        }

        Ok(())
    }
}
