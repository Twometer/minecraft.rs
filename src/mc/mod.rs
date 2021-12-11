// Import the submodules
pub mod buffer;
pub mod client;
pub mod codec;
pub mod proto;
pub mod server;

// Export the public types
pub use self::buffer::ReadBuffer;
pub use self::buffer::WriteBuffer;
pub use self::client::MinecraftClient;
pub use self::server::MinecraftServer;

// Common functions
use std::io::{Read, Result};

pub fn read_var_int(source: &mut impl Read) -> Result<i32> {
    let mut val: i32 = 0;
    let mut buf = [0; 1];
    for i in 0..4 {
        source.read_exact(&mut buf)?;

        let masked = (buf[0] & 0x7f) as i32;
        val |= masked << i * 7;

        if buf[0] & 0x80 == 0 {
            break;
        }
    }
    return Ok(val);
}

pub fn calc_varint_size(mut value: i32) -> usize {
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

pub fn block_pos_to_idx(x: u32, y: u32, z: u32) -> u32 {
    return (y * 16 + z) * 16 + x;
}
