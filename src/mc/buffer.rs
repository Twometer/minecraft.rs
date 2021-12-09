use super::read_var_int;

use inflate::inflate_bytes_zlib;

use std::io::{Cursor, Read};

pub struct WriteBuffer {
    buf: Vec<u8>,
}

pub struct ReadBuffer {
    buf: Cursor<Vec<u8>>,
}

impl WriteBuffer {
    pub fn new() -> WriteBuffer {
        WriteBuffer { buf: Vec::new() }
    }

    pub fn write_varint(&mut self, mut value: i32) {
        loop {
            let mut cur_byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                cur_byte |= 0x80;
            }
            self.buf.push(cur_byte);
            if value == 0 {
                break;
            }
        }
    }

    pub fn write_u8(&mut self, value: u8) {
        let bytes = value.to_be_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_u16(&mut self, value: u16) {
        let bytes = value.to_be_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_u16_le(&mut self, value: u16) {
        let bytes = value.to_le_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_i64(&mut self, value: i64) {
        let bytes = value.to_be_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_i32(&mut self, value: i32) {
        let bytes = value.to_be_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_f32(&mut self, value: f32) {
        let bytes = value.to_be_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_f64(&mut self, value: f64) {
        let bytes = value.to_be_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_string(&mut self, value: &str) {
        self.write_varint(value.len() as i32);
        let bytes = value.as_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_buf(&mut self, other: &WriteBuffer) {
        self.buf.extend(&other.buf);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(&bytes);
    }

    pub fn len(&self) -> usize {
        return self.buf.len();
    }

    pub fn data(&self) -> &[u8] {
        return self.buf.as_slice();
    }
}

impl ReadBuffer {
    pub fn new(vec: Vec<u8>) -> ReadBuffer {
        ReadBuffer {
            buf: Cursor::new(vec),
        }
    }

    pub fn read_var_int(&mut self) -> i32 {
        return read_var_int(&mut self.buf).expect("failed to read var int from buffer");
    }

    pub fn read_u8(&mut self) -> u8 {
        let mut buf = [0; 1];
        self.read_bytes(&mut buf);
        return buf[0];
    }

    pub fn read_u16(&mut self) -> u16 {
        let mut buf = [0; 2];
        self.read_bytes(&mut buf);
        return u16::from_be_bytes(buf);
    }

    pub fn read_u16_le(&mut self) -> u16 {
        let mut buf = [0; 2];
        self.read_bytes(&mut buf);
        return u16::from_le_bytes(buf);
    }

    pub fn read_i32(&mut self) -> i32 {
        let mut buf = [0; 4];
        self.read_bytes(&mut buf);
        return i32::from_be_bytes(buf);
    }

    pub fn read_i64(&mut self) -> i64 {
        let mut buf = [0; 8];
        self.read_bytes(&mut buf);
        return i64::from_be_bytes(buf);
    }

    pub fn read_f32(&mut self) -> f32 {
        let mut buf = [0; 4];
        self.read_bytes(&mut buf);
        return f32::from_be_bytes(buf);
    }

    pub fn read_f64(&mut self) -> f64 {
        let mut buf = [0; 8];
        self.read_bytes(&mut buf);
        return f64::from_be_bytes(buf);
    }

    pub fn read_string(&mut self) -> String {
        let len = self.read_var_int();
        let mut buf = vec![0u8; len as usize];
        self.read_bytes(&mut buf);
        return String::from_utf8(buf).expect("invalid string received");
    }

    pub fn read_bool(&mut self) -> bool {
        return self.read_u8() != 0;
    }

    pub fn skip(&mut self, num: u64) {
        self.buf.set_position(self.buf.position() + num);
    }

    pub fn decompress(&mut self) {
        let mut in_buf = Vec::<u8>::new();
        self.buf
            .read_to_end(&mut in_buf)
            .expect("failed to read from buffer");

        let out_buf = inflate_bytes_zlib(&in_buf).expect("failed to decompress packet");
        self.buf = Cursor::new(out_buf);
    }

    fn read_bytes(&mut self, bytes: &mut [u8]) {
        self.buf
            .read_exact(bytes)
            .expect("failed to read from buffer");
    }
}
