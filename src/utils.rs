use serde_json::json;
use tokio::sync::mpsc;

use crate::mc::proto::Packet;

pub async fn broadcast_chat(broadcast: &mut mpsc::Sender<Packet>, message: String) {
    broadcast
        .send(Packet::S02ChatMessage {
            json_data: json!({ "text": message }).to_string(),
            position: 0,
        })
        .await
        .expect("Failed to send chat message");
}
