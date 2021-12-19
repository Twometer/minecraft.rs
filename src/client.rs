use std::{
    ops::Add,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
    time::Duration,
};

use dashmap::DashSet;
use futures::{SinkExt, StreamExt};
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
    config::ServerConfig,
    mc::{
        codec::MinecraftCodec,
        proto::{AbilityFlags, EntityMetaData, EntityMetaEntry, Packet},
        proto::{PlayState, PlayerListItemAction},
    },
    utils::broadcast_chat,
    world::{sched::GenerationScheduler, Chunk, ChunkPos, MutexChunkRef, World},
};

static EID_COUNTER: AtomicI32 = AtomicI32::new(0);

pub struct ClientHandler {
    in_stream: Framed<TcpStream, MinecraftCodec>,
    out_stream: mpsc::Receiver<Packet>,
    broadcast: mpsc::Sender<Packet>,
    world: Arc<World>,
    world_gen: Arc<GenerationScheduler>,
    server_config: Arc<ServerConfig>,
    entity_id: i32,
    username: String,
    known_chunks: DashSet<ChunkPos>,
    current_chunk_pos: ChunkPos,
    fly_speed: f32,
    walk_speed: f32,
    game_mode: u8,
    player_uuid: Uuid,
}

impl ClientHandler {
    pub fn new(
        in_stream: Framed<TcpStream, MinecraftCodec>,
        out_stream: mpsc::Receiver<Packet>,
        broadcast: mpsc::Sender<Packet>,
        world: Arc<World>,
        world_gen: Arc<GenerationScheduler>,
        server_config: Arc<ServerConfig>,
    ) -> ClientHandler {
        let rand: u128 = rand::thread_rng().gen();
        let game_mode = server_config.gamemode;
        ClientHandler {
            in_stream,
            out_stream,
            broadcast,
            world,
            world_gen,
            server_config,
            entity_id: 0,
            username: String::new(),
            known_chunks: DashSet::new(),
            current_chunk_pos: ChunkPos::new(0, 0),
            fly_speed: 0.05,
            walk_speed: 0.1,
            game_mode,
            player_uuid: Uuid::from_u128(rand),
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
                        "max": self.server_config.slots,
                        "online": 0,
                        "sample": []
                    },
                    "description": {
                        "text": self.server_config.motd
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
                        threshold: self.server_config.net_compression as i32,
                    })
                    .await?;
                self.in_stream
                    .codec_mut()
                    .change_compression_threshold(self.server_config.net_compression);

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
                        gamemode: self.game_mode,
                        dimension: 0,
                        difficulty: self.server_config.difficulty,
                        player_list_size: 4,
                        world_type: "default".to_string(),
                        reduced_debug_info: false,
                    })
                    .await?;

                // Transmit world
                self.send_chunks(0, 0, self.server_config.view_dist).await?;

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
                        gamemode: self.game_mode as i32,
                        display_name: None,
                        ping: 0,
                    },
                })
                .await;
            }
            Packet::C01ChatMessage { message } => {
                if !self.handle_command(message.as_str()).await {
                    let prepared_message = format!("§b{}§r: {}", self.username, message);
                    info!("Chat message: {}", prepared_message);
                    broadcast_chat(&mut self.broadcast, prepared_message).await;
                }
            }
            Packet::C07PlayerDigging {
                location, status, ..
            } => {
                // TODO Sanitize position
                if self.game_mode == 0 {
                    if status == 2 {
                        // Was digging finished?
                        let block = self.world.get_block(location.x, location.y, location.z);
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
                        self.world.set_block(location.x, location.y, location.z, 0);
                    }
                } else {
                    self.world.set_block(location.x, location.y, location.z, 0);
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

    async fn handle_command(&mut self, command: &str) -> bool {
        if !command.starts_with("/") {
            return false;
        }

        let args = &command[1..].split(" ").collect::<Vec<&str>>();
        let command = args[0];
        let args = &args[1..];

        match command {
            "help" => {
                let help_msg = "§a== Help ==§r\n/gm\n/demo";
                self.send_chat_message(json!({ "text": help_msg }), 1).await;
            }
            "gm" => {
                self.change_game_mode(args[0].parse::<u8>().unwrap()).await;
            }
            "speed" => {
                let speed_mul = args[0].parse::<f32>().unwrap();
                self.walk_speed = 0.1 * speed_mul;
                self.fly_speed = 0.05 * speed_mul;
                self.send_abilities().await;
            }
            "demo" => {
                self.send_packet(Packet::S2BChangeGameState {
                    reason: 5,
                    value: 0.0,
                })
                .await;
            }
            _ => {
                let message = format!(
                    "§cUnknown command '/{}'. §rTry /help for a list of commands.",
                    command
                );
                self.send_chat_message(json!({ "text": message }), 1).await
            }
        }

        true
    }

    async fn change_game_mode(&mut self, gamemode: u8) {
        self.game_mode = gamemode;
        self.send_packet(Packet::S2BChangeGameState {
            reason: 3,
            value: gamemode as f32,
        })
        .await;
        self.send_abilities().await;
        self.send_broadcast(Packet::S38PlayerListItem {
            uuid: self.player_uuid,
            action: PlayerListItemAction::UpdateGameMode {
                gamemode: self.game_mode as i32,
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
            flags: AbilityFlags::from_gamemode(self.game_mode),
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

            let r = self.server_config.view_dist;
            self.world_gen.request_region(center.x, center.z, r);
            self.world_gen.await_region(center.x, center.z, r).await;
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
                let chunk_opt = self.world.get_chunk(chunk_pos);
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
