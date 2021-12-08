use std::io::Write;
use std::{io::Read, net::TcpStream};

use log::debug;
use log::info;

use crate::mc::read_var_int;
use crate::mc::ReadBuffer;

use super::{calc_varint_size, WriteBuffer};

pub struct MinecraftConnection {
    stream: TcpStream,
    compression_threshold: usize,
    play_state: PlayState,
}

#[derive(Debug)]
enum PlayState {
    Handshake,
    Status,
    Login,
    Play,
}

impl MinecraftConnection {
    pub fn new(stream: TcpStream) -> MinecraftConnection {
        MinecraftConnection {
            stream,
            compression_threshold: 0,
            play_state: PlayState::Handshake,
        }
    }

    pub fn receive_loop(&mut self) {
        loop {
            let packet_len = read_var_int(&mut self.stream);
            if packet_len <= 0 {
                debug!("Connection lost to {}", self.stream.peer_addr().unwrap());
                return;
            }

            let mut data = vec![0u8; packet_len as usize];
            self.stream
                .read_exact(&mut data)
                .expect("failed to read packet contents");

            let mut buf = ReadBuffer::new(data);
            if self.compression_threshold > 0 {
                let size_uncompressed = buf.read_var_int();
                if size_uncompressed > 0 {
                    buf.decompress();
                }
            }

            let packet_id = buf.read_var_int();
            debug!(
                "Read packet {} with length {} from stream",
                packet_id, packet_len
            );

            match self.play_state {
                PlayState::Handshake => self.handle_handshake_packet(packet_id, &mut buf),
                PlayState::Status => self.handle_status_packet(packet_id, &mut buf),
                PlayState::Login => self.handle_login_packet(packet_id, &mut buf),
                PlayState::Play => self.handle_play_packet(packet_id, &mut buf),
            }
        }
    }

    fn change_state(&mut self, next_state: i32) {
        self.play_state = match next_state {
            1 => PlayState::Status,
            2 => PlayState::Login,
            3 => PlayState::Play,
            _ => panic!("Invalid play state {}", next_state),
        };
        info!("Changed to PlayState::{:?}", self.play_state);
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
                let json = r#"{"version":{"name":"1.8.0","protocol":47},"players":{"max":100,"online":0,"sample":[]},"description":{"text":"Hello from the Rustcraft Server"}}"#;
                response.write_string(json);
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

    fn handle_login_packet(&self, packet_id: i32, buf: &mut ReadBuffer) {}

    fn handle_play_packet(&self, packet_id: i32, buf: &mut ReadBuffer) {}
}
