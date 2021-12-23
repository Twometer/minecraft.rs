use std::{
    ops::Add,
    sync::atomic::{AtomicI32, Ordering},
    time::Duration,
};

use dashmap::DashSet;
use futures::{SinkExt, StreamExt};
use indoc::indoc;
use log::{error, info, trace};
use rand::Rng;
use serde_json::json;
use tokio::{
    net::TcpStream,
    select,
    sync::mpsc,
    time::{self, Instant},
};
use tokio_util::codec::Framed;
use uuid::Uuid;

use crate::{
    block_id, block_meta,
    command::Command,
    mc::{
        codec::MinecraftCodec,
        proto::{
            AbilityFlags, DiggingStatus, EntityMetaData, EntityMetaEntry, GameStateReason, Packet,
        },
        proto::{PlayState, PlayerListItemAction},
    },
    model::{GameMode, Server},
    utils::broadcast_chat,
    world::{Chunk, ChunkPos, MutexChunkRef},
};

static EID_COUNTER: AtomicI32 = AtomicI32::new(0);

pub struct ClientHandler {
    in_stream: Framed<TcpStream, MinecraftCodec>,
    out_stream: mpsc::Receiver<Packet>,
    broadcast: mpsc::Sender<Packet>,
    server: Server,
    entity_id: i32,
    username: String,
    known_chunks: DashSet<ChunkPos>,
    current_chunk_pos: ChunkPos,
    fly_speed: f32,
    walk_speed: f32,
    game_mode: GameMode,
    player_uuid: Uuid,
}

impl ClientHandler {
    pub fn new(
        in_stream: Framed<TcpStream, MinecraftCodec>,
        out_stream: mpsc::Receiver<Packet>,
        broadcast: mpsc::Sender<Packet>,
        server: Server,
    ) -> ClientHandler {
        let game_mode = server.config.game_mode;
        ClientHandler {
            in_stream,
            out_stream,
            broadcast,
            server,
            entity_id: 0,
            username: String::new(),
            known_chunks: DashSet::new(),
            current_chunk_pos: ChunkPos::new(0, 0),
            fly_speed: 0.05,
            walk_speed: 0.1,
            game_mode,
            player_uuid: Uuid::from_u128(rand::thread_rng().gen()),
        }
    }

    pub async fn loop_until_disconnect(&mut self) {
        let mut keep_alive_interval = time::interval_at(
            Instant::now().add(Duration::from_secs(5)),
            Duration::from_secs(10),
        );

        loop {
            select! {
                packet_in = self.in_stream.next() => {
                    if packet_in.is_none() {
                        break;
                    }

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
                packet_out = self.out_stream.recv() => {
                    if packet_out.is_none() {
                        break;
                    }

                    self.in_stream.send(packet_out.unwrap()).await.expect("Client send failed");
                }
                _ = keep_alive_interval.tick() => {
                    self.in_stream
                        .send(Packet::S00KeepAlive { timestamp: 69 })
                        .await
                        .expect("Client keep-alive failed");
                }
            }
        }

        self.in_stream.close().await.unwrap();
        self.out_stream.close();
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

                self.in_stream.codec_mut().change_state(next_state);
            }

            Packet::C00StatusRequest => {
                let status = json!({
                    "version": {
                        "name": "1.8.0",
                        "protocol": 47
                    },
                    "players":{
                        "max": self.server.config.slots,
                        "online": 0,
                        "sample": []
                    },
                    "description": {
                        "text": self.server.config.motd
                    }
                });
                self.in_stream
                    .send(Packet::S00StatusResponse {
                        status: status.to_string(),
                    })
                    .await?;
            }
            Packet::C01StatusPing { timestamp } => {
                self.in_stream
                    .send(Packet::S01StatusPong { timestamp })
                    .await?;
            }

            Packet::C00LoginStart { username } => {
                self.username = username;

                // Enable compression
                self.in_stream
                    .send(Packet::S03LoginCompression {
                        threshold: self.server.config.net_compression as i32,
                    })
                    .await?;
                self.in_stream
                    .codec_mut()
                    .change_compression_threshold(self.server.config.net_compression);

                // Enter play state
                self.in_stream
                    .send(Packet::S02LoginSuccess {
                        uuid: self.player_uuid.to_string(),
                        username: self.username.clone(),
                    })
                    .await?;
                self.in_stream.codec_mut().change_state(PlayState::Play);

                // Complete login sequence
                self.entity_id = EID_COUNTER.fetch_add(1, Ordering::SeqCst);
                self.in_stream
                    .send(Packet::S01JoinGame {
                        entity_id: self.entity_id,
                        game_mode: self.game_mode,
                        dimension: 0,
                        difficulty: self.server.config.difficulty,
                        player_list_size: 4,
                        world_type: "default".to_string(),
                        reduced_debug_info: false,
                    })
                    .await?;

                // Transmit world
                self.send_chunks(0, 0, self.server.config.view_dist).await?;

                // Spawn player into world
                self.in_stream
                    .send(Packet::S08SetPlayerPosition {
                        x: 0.0,
                        y: 64.0,
                        z: 0.0,
                        yaw: 0.0,
                        pitch: 0.0,
                        flags: 0,
                    })
                    .await?;

                // Announce login
                info!(
                    "{} logged in with entity id {}",
                    self.username, self.entity_id
                );
                broadcast_chat(
                    &mut self.broadcast,
                    format!("§e{} joined the game", self.username),
                )
                .await;
                self.send_broadcast(Packet::S38PlayerListItem {
                    uuid: self.player_uuid,
                    action: PlayerListItemAction::AddPlayer {
                        name: self.username.clone(),
                        game_mode: self.game_mode,
                        display_name: None,
                        ping: 0,
                    },
                })
                .await;
            }
            Packet::C01ChatMessage { message } => {
                let message = message.as_str();
                if message.starts_with("/") {
                    self.handle_command(message).await;
                } else {
                    info!("Chat message: <{}> {}", self.username, message);

                    let formatted_message = format!("§b{}§r: {}", self.username, message);
                    broadcast_chat(&mut self.broadcast, formatted_message).await;
                }
            }
            Packet::C07PlayerDigging {
                location, status, ..
            } => {
                // TODO Sanitize position
                if self.game_mode == GameMode::Survival {
                    if status == DiggingStatus::FinishDigging {
                        let block = self
                            .server
                            .world
                            .get_block(location.x, location.y, location.z);
                        let block_id = block_id!(block);
                        let block_meta = block_meta!(block);

                        // Create item entity
                        let dropped_item_eid = EID_COUNTER.fetch_add(1, Ordering::SeqCst);
                        self.in_stream
                            .send(Packet::S0ESpawnObject {
                                entity_id: dropped_item_eid,
                                kind: 2,
                                x: location.x as f32 + 0.5,
                                y: location.y as f32 + 0.5,
                                z: location.z as f32 + 0.5,
                                pitch: 0.0,
                                yaw: 0.0,
                                data: 0,
                            })
                            .await?;

                        // Set item entity metadata
                        self.in_stream
                            .send(Packet::S1CEntityMeta {
                                entity_id: dropped_item_eid,
                                entries: vec![EntityMetaEntry::new(
                                    10,
                                    EntityMetaData::Slot {
                                        id: block_id,
                                        count: 1,
                                        damage: block_meta,
                                    },
                                )],
                            })
                            .await?;

                        // Set block in world
                        self.server
                            .world
                            .set_block(location.x, location.y, location.z, 0);
                    }
                } else {
                    self.server
                        .world
                        .set_block(location.x, location.y, location.z, 0);
                }
            }
            Packet::C04PlayerPos { x, z, .. } => {
                self.update_chunks(ChunkPos::from_block_pos(x as i32, z as i32))
                    .await?;
            }
            Packet::C06PlayerPosRot { x, z, .. } => {
                self.update_chunks(ChunkPos::from_block_pos(x as i32, z as i32))
                    .await?;
            }
            _ => {
                trace!("Received unhandled packet: {:?}", packet);
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: &str) {
        let result = self.process_command(command).await;
        let message_opt = match result {
            Ok(str) => str,
            Err(str) => Some(format!("§cError: {}", str)),
        };
        if message_opt.is_some() {
            self.send_chat_message(json!({"text": message_opt.unwrap()}), 1)
                .await;
        }
    }

    async fn process_command(&mut self, command: &str) -> Result<Option<String>, String> {
        let command = Command::parse(command);
        match command.name() {
            "help" => {
                let help_msg = indoc! {"
                == §aHelp§r ==
                §9 /help§r: Show command overview
                §9 /gm §7<mode>§r: Change gamemode
                "};
                return Ok(Some(help_msg.trim().to_string()));
            }
            "gm" => {
                self.change_game_mode(GameMode::from(command.arg::<u8>(0)?))
                    .await;
            }
            _ => return Err(format!("{}: Unknown command.", command.name())),
        }
        Ok(None)
    }

    async fn change_game_mode(&mut self, game_mode: GameMode) {
        self.game_mode = game_mode;
        self.send_packet(Packet::S2BChangeGameState {
            reason: GameStateReason::ChangeGameMode,
            value: game_mode as i32 as f32,
        })
        .await;
        self.send_abilities().await;
        self.send_broadcast(Packet::S38PlayerListItem {
            uuid: self.player_uuid,
            action: PlayerListItemAction::UpdateGameMode {
                game_mode: game_mode,
            },
        })
        .await;
    }

    async fn send_broadcast(&self, packet: Packet) {
        self.broadcast
            .send(packet)
            .await
            .expect("Failed to send broadcast");
    }

    async fn send_abilities(&mut self) {
        self.send_packet(Packet::S39PlayerAbilities {
            flags: AbilityFlags::from_game_mode(self.game_mode),
            flying_speed: self.fly_speed,
            walking_speed: self.walk_speed,
        })
        .await;
    }

    async fn send_chat_message(&mut self, data: serde_json::Value, position: u8) {
        self.send_packet(Packet::S02ChatMessage {
            json_data: data.to_string(),
            position,
        })
        .await;
    }

    async fn send_packet(&mut self, packet: Packet) {
        self.in_stream
            .send(packet)
            .await
            .expect("Failed to send packet");
    }

    async fn update_chunks(&mut self, center: ChunkPos) -> std::io::Result<()> {
        if center != self.current_chunk_pos {
            self.current_chunk_pos = center;

            let r = self.server.config.view_dist;
            self.server.gen.request_region(center.x, center.z, r);
            self.server.gen.await_region(center.x, center.z, r).await;
            self.send_chunks(center.x, center.z, r).await?;

            let min_x = center.x - r;
            let min_z = center.z - r;
            let max_x = center.x + r;
            let max_z = center.z + r;

            let removed = self
                .known_chunks
                .iter()
                .filter(|k| k.x < min_x || k.z < min_z || k.x > max_x || k.z > max_z)
                .map(|k| *k)
                .collect::<Vec<ChunkPos>>();

            for r in removed {
                self.in_stream
                    .send(Packet::S21ChunkData { x: r.x, z: r.z })
                    .await?;
                self.known_chunks.remove(&r);
            }
        }

        Ok(())
    }

    async fn send_chunks(&mut self, center_x: i32, center_z: i32, r: i32) -> std::io::Result<()> {
        let mut chunk_refs = Vec::<MutexChunkRef>::new();

        // Collect chunks to be sent
        for z in -r..=r {
            for x in -r..=r {
                let chunk_pos = ChunkPos::new(center_x + x, center_z + z);
                let chunk_opt = self.server.world.get_chunk(chunk_pos);
                if chunk_opt.is_some() && !self.known_chunks.contains(&chunk_pos) {
                    chunk_refs.push(chunk_opt.unwrap());
                    self.known_chunks.insert(chunk_pos);
                }
            }
        }

        // Split into packets
        let chunks_per_packet = 10;

        for subslice in chunk_refs.chunks(chunks_per_packet) {
            let mut transmit_chunks = Vec::<Chunk>::new();
            for chunk_ref in subslice {
                transmit_chunks.push(chunk_ref.lock().unwrap().clone())
            }

            self.in_stream
                .send(Packet::S26MapChunkBulk {
                    skylight: true,
                    chunks: transmit_chunks,
                })
                .await?;
        }

        Ok(())
    }
}
