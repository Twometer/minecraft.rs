#[macro_export]
macro_rules! chat_packet {
    ($pos: expr, $msg: expr) => {
        Packet::S02ChatMessage {
            json_data: json!({ "text": $msg }).to_string(),
            position: $pos,
        }
    };
}
