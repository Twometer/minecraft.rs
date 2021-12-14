use bytes::BytesMut;
use deflate::deflate_bytes_zlib;
use inflate::inflate_bytes_zlib;

pub fn compress(data: &[u8]) -> Vec<u8> {
    let compressed = deflate_bytes_zlib(data);
    return compressed;
}

pub fn decompress(data: &[u8]) -> BytesMut {
    let decompressed = inflate_bytes_zlib(data).expect("Failed to decompress packet");
    return BytesMut::from(&decompressed[..]);
}
