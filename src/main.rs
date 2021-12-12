mod mc;

use std::time::SystemTime;

use crate::mc::codec::MinecraftCodec;
use futures::{SinkExt, StreamExt};
use log::{error, info, trace};
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::Framed;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    pretty_env_logger::init();
    info!("Starting server...");

    let listener = TcpListener::bind("127.0.0.1:25565").await?;
    info!("Listener bound and ready");

    loop {
        let (stream, _) = listener.accept().await?;
        handle_client(stream);
    }
}

// TODO: Clean this up
fn handle_client(stream: TcpStream) {
    tokio::spawn(async move {
        info!("Client {:?} connected", stream.peer_addr().unwrap());

        let codec = MinecraftCodec::new();
        let mut framed = Framed::new(stream, codec);
        let mut last_keep_alive = SystemTime::now();

        while let Some(f) = framed.next().await {
            match f {
                Ok(packet) => {
                    trace!("Received {:?}", packet);

                    match packet {
                        mc::proto::Packet::C00Handshake {
                            protocol_version,
                            next_state,
                            ..
                        } => {
                            if protocol_version != 47 {
                                panic!("Unsupported protocol version");
                            }

                            framed.codec_mut().change_state(next_state);
                        }
                        mc::proto::Packet::C01StatusPing { timestamp } => {
                            framed
                                .send(mc::proto::Packet::S01StatusPong { timestamp })
                                .await
                                .unwrap();
                        }
                        mc::proto::Packet::C00StatusRequest => {
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
                                    "text": "Hello from §6minecraft.rs §rwith §aT§bo§ck§di§eo"
                                }
                            });
                            framed
                                .send(mc::proto::Packet::S00StatusResponse {
                                    status: status.to_string(),
                                })
                                .await
                                .unwrap();
                        }
                        mc::proto::Packet::C00LoginStart { username } => {
                            framed
                                .send(mc::proto::Packet::S03LoginCompression { threshold: 8192 })
                                .await
                                .unwrap();
                            framed.codec_mut().change_compression_threshold(8192);

                            framed
                                .send(mc::proto::Packet::S02LoginSuccess {
                                    uuid: "3b9f9997-d547-4f70-a37c-8fffbe706002".to_string(),
                                    username,
                                })
                                .await
                                .unwrap();
                            framed.codec_mut().change_state(mc::proto::PlayState::Play);

                            framed
                                .send(mc::proto::Packet::S01JoinGame {
                                    entity_id: 0,
                                    gamemode: 1,
                                    dimension: 0,
                                    difficulty: 0,
                                    player_list_size: 4,
                                    world_type: "default".to_string(),
                                    reduced_debug_info: false,
                                })
                                .await
                                .unwrap();

                            // TODO Transmit the actual world here
                            framed
                                .send(mc::proto::Packet::S26MapChunkBulk {})
                                .await
                                .unwrap();

                            framed
                                .send(mc::proto::Packet::S08SetPlayerPosition {
                                    x: 0.0,
                                    y: 64.0,
                                    z: 0.0,
                                    yaw: 0.0,
                                    pitch: 0.0,
                                    flags: 0,
                                })
                                .await
                                .unwrap();
                        }
                        mc::proto::Packet::C01ChatMessage { message } => {
                            info!("Chat message: {}", message);
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("Client disconnect: {}", e);
                    break;
                }
            }

            let keep_alive_timeout = SystemTime::now().duration_since(last_keep_alive).unwrap();
            if keep_alive_timeout.as_secs() > 10 {
                framed
                    .send(mc::proto::Packet::S00KeepAlive { timestamp: 69 })
                    .await
                    .unwrap();

                last_keep_alive = SystemTime::now();
            }
        }
    });
}
