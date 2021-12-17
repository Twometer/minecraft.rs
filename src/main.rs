mod broker;
mod client;
mod config;
mod mc;
mod utils;
mod world;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{debug, info};
use stopwatch::Stopwatch;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

use crate::broker::PacketBroker;
use crate::client::ClientHandler;
use crate::config::{ServerConfig, WorldGenConfig};
use crate::mc::{codec::MinecraftCodec, proto::Packet};
use crate::world::sched::GenerationScheduler;
use crate::world::{gen::WorldGenerator, World};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let startup_sw = Stopwatch::start_new();

    pretty_env_logger::init();
    info!("Starting server...");

    let server_conf = Arc::new(ServerConfig::load("config/server.toml"));
    debug!("Loaded config: {:?}", server_conf);

    info!("Preparing spawn region...");
    let gen_sw = Stopwatch::start_new();
    let world = Arc::new(World::new());
    let gen = init_world_gen(&server_conf, &world);
    gen.request_region(0, 0, server_conf.view_dist);
    gen.await_region(0, 0, server_conf.view_dist).await;
    info!("Finished generating after {:?}", gen_sw.elapsed());

    info!("Starting listener");
    let listener = TcpListener::bind(server_conf.net_endpoint.as_str()).await?;
    let mut broker = PacketBroker::new();

    info!("Done. Server started in {:?}", startup_sw.elapsed());

    loop {
        let (stream, _) = listener.accept().await?;
        handle_client(
            stream,
            broker.new_unicast().await,
            broker.new_broadcast(),
            world.clone(),
            gen.clone(),
            server_conf.clone(),
        );
    }
}

fn random_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32
}

fn init_world_gen(server_conf: &ServerConfig, world: &Arc<World>) -> Arc<GenerationScheduler> {
    let world_gen_conf = WorldGenConfig::load("config/world.toml");
    debug!("Loaded config: {:?}", world_gen_conf);

    let seed = match server_conf.seed {
        Some(seed) => seed,
        None => random_seed(),
    };
    debug!("Creating world with seed {}", seed);

    let gen = Arc::new(WorldGenerator::new(seed, world_gen_conf, world.clone()));
    let sched = Arc::new(GenerationScheduler::new(world.clone(), gen.clone()));
    sched.start(server_conf.generator_threads);

    sched
}

fn handle_client(
    in_stream: TcpStream,
    out_stream: mpsc::Receiver<Packet>,
    broadcast: mpsc::Sender<Packet>,
    world: Arc<World>,
    world_gen: Arc<GenerationScheduler>,
    server_config: Arc<ServerConfig>,
) {
    tokio::spawn(async move {
        let client_addr = in_stream.peer_addr().unwrap();
        debug!("Client {:?} connected", client_addr);

        let codec = MinecraftCodec::new();
        let framed = Framed::new(in_stream, codec);

        let mut handler = ClientHandler::new(
            framed,
            out_stream,
            broadcast,
            world,
            world_gen,
            server_config,
        );
        handler.loop_until_disconnect().await;

        debug!("Client {:?} disconnected", client_addr);
    });
}
