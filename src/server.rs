use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};

use dashmap::DashMap;
use tokio::{io, sync::mpsc};

use crate::{
    config::ServerConfig,
    mc::proto::Packet,
    world::{sched::GenerationScheduler, World},
};

#[derive(Debug)]
pub enum GameEvent {}

pub struct ServerHandler {
    pub config: Arc<ServerConfig>,
    pub world: Arc<World>,
    pub gen: Arc<GenerationScheduler>,
    broadcast_tx: mpsc::Sender<Packet>,
    clients: DashMap<i32, mpsc::Sender<Packet>>,
    id_counter: AtomicI32,
    player_counter: AtomicI32,
}

impl ServerHandler {
    pub fn start(
        config: Arc<ServerConfig>,
        world: Arc<World>,
        gen: Arc<GenerationScheduler>,
    ) -> Arc<ServerHandler> {
        let (broadcast_tx, broadcast_rx) = mpsc::channel::<Packet>(128);

        let handler = Arc::new(ServerHandler {
            config,
            world,
            gen,
            broadcast_tx,
            clients: DashMap::new(),
            id_counter: AtomicI32::new(1),
            player_counter: AtomicI32::new(0),
        });

        let h = handler.clone();
        tokio::spawn(async move {
            h.run_broker_loop(broadcast_rx).await;
        });

        handler
    }

    pub fn new_id(&self) -> i32 {
        self.id_counter.fetch_add(1, Ordering::SeqCst)
    }

    pub fn add_client(&self, id: i32) -> mpsc::Receiver<Packet> {
        let (tx, rx) = mpsc::channel::<Packet>(128);
        self.clients.insert(id, tx);
        rx
    }

    pub fn remove_client(&self, id: i32) {
        self.clients.remove(&id);
    }

    pub fn change_num_players(&self, chg: i32) {
        self.player_counter.fetch_add(chg, Ordering::SeqCst);
    }

    pub fn num_players(&self) -> i32 {
        self.player_counter.load(Ordering::SeqCst)
    }

    pub async fn send_broadcast(&self, packet: Packet) -> io::Result<()> {
        match self.broadcast_tx.send(packet).await {
            Ok(_) => Ok(()),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }

    async fn run_broker_loop(&self, mut rx: mpsc::Receiver<Packet>) {
        while let Some(packet) = rx.recv().await {
            for c in &self.clients {
                c.send(packet.clone())
                    .await
                    .expect("Failed to send packet to client");
            }
        }
    }
}
