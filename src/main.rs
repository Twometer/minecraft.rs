mod broker;
mod client;
mod config;
mod mc;
mod utils;
mod world;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{debug, info};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

use crate::broker::PacketBroker;
use crate::client::ClientHandler;
use crate::config::WorldGenConfig;
use crate::mc::{codec::MinecraftCodec, proto::Packet};
use crate::world::{gen::WorldGenerator, World};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    pretty_env_logger::init();
    info!("Starting server...");

    let listener = TcpListener::bind("127.0.0.1:25565").await?;
    info!("Listener bound and ready");

    let world_gen_conf = WorldGenConfig::load("config/world.toml");
    debug!("Loaded config {:?}", world_gen_conf);

    info!("Preparing spawn region...");
    let start = SystemTime::now();
    let world = Arc::new(World::new());

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    let gen = WorldGenerator::new(seed, world_gen_conf, world.clone());
    gen.generate_spawn();
    let duration = SystemTime::now().duration_since(start).unwrap();
    info!("Done generating spawn region after {:?}", duration);

    let mut broker = PacketBroker::new();
    loop {
        let (stream, _) = listener.accept().await?;
        handle_client(
            stream,
            broker.new_unicast().await,
            broker.new_broadcast(),
            world.clone(),
        );
    }
}

fn handle_client(
    in_stream: TcpStream,
    out_stream: mpsc::Receiver<Packet>,
    broadcast: mpsc::Sender<Packet>,
    world: Arc<World>,
) {
    tokio::spawn(async move {
        let client_addr = in_stream.peer_addr().unwrap();
        debug!("Client {:?} connected", client_addr);

        let codec = MinecraftCodec::new();
        let framed = Framed::new(in_stream, codec);

        let mut handler = ClientHandler::new(framed, out_stream, broadcast, world);
        handler.handle_loop().await;

        debug!("Client {:?} disconnected", client_addr);
    });
}
