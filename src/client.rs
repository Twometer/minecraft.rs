use std::time::Duration;

use crate::mc::{codec::MinecraftCodec, proto::Packet, proto::PlayState};
use futures::{SinkExt, StreamExt};
use log::{error, info, trace};
use serde_json::json;
use tokio::{net::TcpStream, select, sync::mpsc, time};
use tokio_util::codec::Framed;

pub struct ClientHandler {
    stream: Framed<TcpStream, MinecraftCodec>,
    broadcast: mpsc::Sender<Packet>,
    transmit: mpsc::Receiver<Packet>,
}

impl ClientHandler {
    pub fn new(
        stream: Framed<TcpStream, MinecraftCodec>,
        broadcast: mpsc::Sender<Packet>,
        transmit: mpsc::Receiver<Packet>,
    ) -> ClientHandler {
        ClientHandler {
            stream,
            broadcast,
            transmit,
        }
    }

    pub async fn handle_loop(&mut self) {
        let mut keep_alive_interval = time::interval(Duration::from_secs(10));

        loop {
            select! {
                packet_in = self.stream.next() => {

                    match packet_in.unwrap() {
                        Ok(packet) => {
                            self.handle_packet(packet)
                                .await
                                .expect("Packet handler failed");
                        }
                        Err(err) => {
                            error!("Client receive failed: {}", err);
                            break;
                        }
                    }

                },
                packet_out = self.transmit.recv() => {
                    self.stream.send(packet_out.unwrap()).await.expect("Client send failed");
                }
                _ = keep_alive_interval.tick() => {
                    self.stream
                        .send(Packet::S00KeepAlive { timestamp: 69 })
                        .await
                        .expect("Client keep-alive failed");
                }
            }
        }
    }

    async fn handle_packet(&mut self, packet: Packet) -> std::io::Result<()> {
        trace!("Received {:?}", packet);

        match packet {
            Packet::C00Handshake {
                protocol_version,
                next_state,
                ..
            } => {
                if protocol_version != 47 {
                    panic!("Unsupported protocol version");
                }

                self.stream.codec_mut().change_state(next_state);
            }

            Packet::C00StatusRequest => {
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
                self.stream
                    .send(Packet::S00StatusResponse {
                        status: status.to_string(),
                    })
                    .await?;
            }
            Packet::C01StatusPing { timestamp } => {
                self.stream
                    .send(Packet::S01StatusPong { timestamp })
                    .await?;
            }

            Packet::C00LoginStart { username } => {
                self.stream
                    .send(Packet::S03LoginCompression { threshold: 8192 })
                    .await?;
                self.stream.codec_mut().change_compression_threshold(8192);

                self.stream
                    .send(Packet::S02LoginSuccess {
                        uuid: "3b9f9997-d547-4f70-a37c-8fffbe706002".to_string(),
                        username,
                    })
                    .await?;
                self.stream.codec_mut().change_state(PlayState::Play);

                self.stream
                    .send(Packet::S01JoinGame {
                        entity_id: 0,
                        gamemode: 1,
                        dimension: 0,
                        difficulty: 0,
                        player_list_size: 4,
                        world_type: "default".to_string(),
                        reduced_debug_info: false,
                    })
                    .await?;

                // TODO Transmit the actual world here
                self.stream.send(Packet::S26MapChunkBulk {}).await?;

                self.stream
                    .send(Packet::S08SetPlayerPosition {
                        x: 0.0,
                        y: 64.0,
                        z: 0.0,
                        yaw: 0.0,
                        pitch: 0.0,
                        flags: 0,
                    })
                    .await?;
            }
            Packet::C01ChatMessage { message } => {
                self.broadcast
                    .send(Packet::S02ChatMessage {
                        json_data: json!({ "text": message }).to_string(),
                        position: 0,
                    })
                    .await
                    .unwrap();
                info!("Chat message: {}", message);
            }

            _ => {
                trace!("Received unhandled packet: {:?}", packet);
            }
        }

        Ok(())
    }
}
