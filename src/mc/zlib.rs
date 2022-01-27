use std::io::{Read, Write};

use bytes::BytesMut;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
//use inflate::inflate_bytes_zlib;

pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("Failed to write to ZLib");
    return encoder.finish().expect("Failed to encode ZLib");
}

pub fn decompress(data: &[u8]) -> BytesMut {
    let mut out_vec = Vec::new();
    let mut decoder = ZlibDecoder::new(data);
    decoder
        .read_to_end(&mut out_vec)
        .expect("Failed to decode ZLib");
    return BytesMut::from(&out_vec[..]);
}
