use std::sync::atomic::{AtomicI32};
use std::{io::Read, io::Result, io::Write, net::TcpStream};

use log::info;
use log::{debug, trace};
use serde_json::json;

use crate::mc::read_var_int;
use crate::mc::ReadBuffer;

use super::{calc_varint_size, WriteBuffer};

static eid_counter: AtomicI32 = AtomicI32::new(0);

pub struct MinecraftClient {
    stream: TcpStream,
    compression_threshold: usize,
    play_state: PlayState,
    entity_id: i32,
    username: String,
}

#[derive(Debug)]
enum PlayState {
    Handshake,
    Status,
    Login,
    Play,
}

impl MinecraftClient {
    pub fn new(stream: TcpStream) -> MinecraftClient {
        MinecraftClient {
            stream,
            compression_threshold: 0,
            play_state: PlayState::Handshake,
            entity_id: 0,
            username: String::new(),
        }
    }

    pub fn receive_loop(&mut self) {
        loop {
            let result = self.read_packet();
            if result.is_err() {
                debug!(
                    "Connection with {} closed: {}",
                    self.stream.peer_addr().unwrap(),
                    result.unwrap_err()
                );
                return;
            }
        }
    }

    fn read_packet(&mut self) -> Result<()> {
        let packet_len = read_var_int(&mut self.stream)?;

        let mut data = vec![0u8; packet_len as usize];
        self.stream.read_exact(&mut data)?;

        let mut buf = ReadBuffer::new(data);
        if self.compression_threshold > 0 {
            let size_uncompressed = buf.read_var_int();
            if size_uncompressed > 0 {
                buf.decompress();
            }
        }

        let packet_id = buf.read_var_int();
        trace!(
            "Read packet {} with length {} from stream",
            packet_id,
            packet_len
        );

        match self.play_state {
            PlayState::Handshake => self.handle_handshake_packet(packet_id, &mut buf),
            PlayState::Status => self.handle_status_packet(packet_id, &mut buf),
            PlayState::Login => self.handle_login_packet(packet_id, &mut buf),
            PlayState::Play => self.handle_play_packet(packet_id, &mut buf),
        }
        Ok(())
    }

    fn change_state(&mut self, next_state: i32) {
        self.play_state = match next_state {
            1 => PlayState::Status,
            2 => PlayState::Login,
            3 => PlayState::Play,
            _ => panic!("Invalid play state {}", next_state),
        };
        debug!("Changed to PlayState::{:?}", self.play_state);
    }

    fn send_packet(&mut self, id: i32, payload: &WriteBuffer) {
        let mut packet = WriteBuffer::new();

        if self.compression_threshold > 0 {
            // FIXME: Actual compression!
            let packet_len = calc_varint_size(id) + calc_varint_size(0) + payload.len();
            packet.write_varint(packet_len as i32);
            packet.write_varint(0);
        } else {
            let packet_len = calc_varint_size(id) + payload.len();
            packet.write_varint(packet_len as i32);
        }

        packet.write_varint(id);
        packet.write_buf(payload);

        self.stream
            .write(packet.data())
            .expect("failed to send packet");
    }

    fn handle_handshake_packet(&mut self, packet_id: i32, buf: &mut ReadBuffer) {
        if packet_id == 0 {
            let protocol_version = buf.read_var_int();
            let server_address = buf.read_string();
            let server_port = buf.read_u16();
            let next_state = buf.read_var_int();

            if protocol_version != 47 {
                panic!("Unsupported protocol version");
            }

            self.change_state(next_state);
        }
    }

    fn handle_status_packet(&mut self, packet_id: i32, buf: &mut ReadBuffer) {
        match packet_id {
            0x00 => {
                // Request
                let mut response = WriteBuffer::new();
                let status = json!({
                    "version": {
                        "name": "1.8.0",
                        "protocol": 47
                    },
                    "players":{
                        "max": 20,
                        "online": 0,
                        "sample": []
                    },
                    "description": {
                        "text": "Hello from §6minecraft.rs"
                    }
                });
                response.write_string(status.to_string().as_str());
                self.send_packet(0x00, &response)
            }
            0x01 => {
                // Ping
                let ping_payload = buf.read_i64();
                let mut response = WriteBuffer::new();
                response.write_i64(ping_payload);
                self.send_packet(0x01, &response);
            }
            _ => panic!("Received invalid packet {}", packet_id),
        }
    }

    fn handle_login_packet(&mut self, packet_id: i32, buf: &mut ReadBuffer) {
        match packet_id {
            0x00 => {
                // Decode 'Login start'
                let username = buf.read_string();
                self.username = username.to_string();

                // Send 'Set compression'
                let mut set_compression = WriteBuffer::new();
                set_compression.write_varint(8192);
                self.send_packet(0x03, &set_compression);
                self.compression_threshold = 8192;

                // Send 'Login success'
                let mut response = WriteBuffer::new();
                response.write_string("3b9f9997-d547-4f70-a37c-8fffbe706002"); // TODO: Use correct UUID
                response.write_string(&username);
                self.send_packet(0x02, &response);

                // Change play state
                self.change_state(PlayState::Play as i32);

                self.entity_id = eid_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                info!(
                    "Player {} logged in with entity id {}",
                    username, self.entity_id
                );

                // Send 'Join Game'
                let mut join_game = WriteBuffer::new();
                join_game.write_i32(self.entity_id);
                join_game.write_u8(1); // Gamemode creative
                join_game.write_u8(0); // Overworld
                join_game.write_u8(0); // Peaceful
                join_game.write_u8(4); // Size of player list
                join_game.write_string("default");
                join_game.write_u8(0);
                self.send_packet(0x01, &join_game);

                // Send spawn position
                let mut spawn_pos = WriteBuffer::new();
                spawn_pos.write_f64(0.0); // X
                spawn_pos.write_f64(0.0); // Y
                spawn_pos.write_f64(0.0); // Z
                spawn_pos.write_f32(0.0); // Yaw
                spawn_pos.write_f32(0.0); // Pitch
                spawn_pos.write_u8(0); // Flags
                self.send_packet(0x08, &spawn_pos);
            }
            _ => panic!("Received invalid packet {}", packet_id),
        }
    }

    fn handle_play_packet(&mut self, packet_id: i32, buf: &mut ReadBuffer) {
        match packet_id {
            0x01 => {
                let chat_message = buf.read_string();
                info!("<{}> {}", self.username, chat_message);

                let mut response = WriteBuffer::new();
                let chat_obj = json!({ "text": format!("<{}> {}", self.username, chat_message) });
                response.write_string(chat_obj.to_string().as_str());
                response.write_u8(0);
                self.send_packet(0x02, &response);
            }
            _ => {}
        }
    }
}
