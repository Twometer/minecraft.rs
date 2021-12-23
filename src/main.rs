mod broker;
mod client;
mod command;
mod config;
mod mc;
mod model;
mod utils;
mod world;

use std::sync::Arc;

use log::{debug, info};
use model::Server;
use stopwatch::Stopwatch;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

use crate::broker::PacketBroker;
use crate::client::ClientHandler;
use crate::config::{ServerConfig, WorldGenConfig};
use crate::mc::{codec::MinecraftCodec, proto::Packet};
use crate::world::random_seed;
use crate::world::sched::GenerationScheduler;
use crate::world::{gen::WorldGenerator, World};

const SERVER_CONFIG_PATH: &str = "config/server.toml";
const WORLD_CONFIG_PATH: &str = "config/world.toml";

#[tokio::main]
async fn main() -> io::Result<()> {
    pretty_env_logger::init();

    info!("Starting server...");
    let startup_sw = Stopwatch::start_new();
    let server = create_server();

    info!("Preparing spawn region...");
    let gen_sw = Stopwatch::start_new();
    server.gen.request_region(0, 0, server.config.view_dist);
    server.gen.await_region(0, 0, server.config.view_dist).await;
    info!("Spawn region prepared in {:?}", gen_sw.elapsed());

    info!("Binding TCP listener...");
    let listener = TcpListener::bind(server.config.net_endpoint.as_str()).await?;
    let mut broker = PacketBroker::new();

    info!("Done. Server started in {:?}", startup_sw.elapsed());

    loop {
        let (stream, _) = listener.accept().await?;
        handle_client(
            stream,
            broker.new_unicast().await,
            broker.new_broadcast(),
            server.clone(),
        );
    }
}

fn create_server() -> Server {
    let config = Arc::new(ServerConfig::load(SERVER_CONFIG_PATH));
    debug!("Loaded config: {:?}", config);

    let world = Arc::new(World::new());
    let gen = create_world_gen(&config, &world);
    Server::new(config, world, gen)
}

fn create_world_gen(
    server_conf: &Arc<ServerConfig>,
    world: &Arc<World>,
) -> Arc<GenerationScheduler> {
    let config = WorldGenConfig::load(WORLD_CONFIG_PATH);
    debug!("Loaded config: {:?}", config);

    let seed = match server_conf.seed {
        Some(seed) => seed,
        None => random_seed(),
    };
    debug!("Initializing world generator with seed {}", seed);

    let gen = Arc::new(WorldGenerator::new(seed, config, world.clone()));
    let sched = Arc::new(GenerationScheduler::new(world.clone(), gen));
    sched.start(server_conf.generator_threads);

    sched
}

fn handle_client(
    in_stream: TcpStream,
    out_stream: mpsc::Receiver<Packet>,
    broadcast: mpsc::Sender<Packet>,
    server: Server,
) {
    tokio::spawn(async move {
        let client_addr = in_stream.peer_addr().unwrap();
        debug!("Client {:?} connected", client_addr);

        let codec = MinecraftCodec::new();
        let in_stream = Framed::new(in_stream, codec);

        let mut handler = ClientHandler::new(in_stream, out_stream, broadcast, server);
        handler.loop_until_disconnect().await;

        debug!("Client {:?} disconnected", client_addr);
    });
}
