use std::{io::Read, net::TcpStream};

use log::debug;

use crate::mc::read_var_int;
use crate::mc::ReadBuffer;

pub struct MinecraftConnection {
    stream: TcpStream,
    compression_threshold: usize,
}

impl MinecraftConnection {
    pub fn new(stream: TcpStream) -> MinecraftConnection {
        MinecraftConnection {
            stream,
            compression_threshold: 0,
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
        }
    }
}
