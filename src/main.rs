mod client;
mod command;
mod config;
mod mc;
mod model;
mod server;
mod utils;
mod world;

use std::sync::Arc;

use log::{debug, info};
use stopwatch::Stopwatch;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

use crate::client::ClientHandler;
use crate::config::{ServerConfig, WorldGenConfig};
use crate::mc::{codec::MinecraftCodec, proto::Packet};
use crate::server::ServerHandler;
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

    info!("Done. Server started in {:?}", startup_sw.elapsed());

    loop {
        let (stream, _) = listener.accept().await?;
        let client_id = server.new_id();
        handle_client(
            client_id,
            stream,
            server.add_client(client_id),
            server.clone(),
        );
    }
}

fn create_server() -> Arc<ServerHandler> {
    let config = Arc::new(ServerConfig::load(SERVER_CONFIG_PATH));
    debug!("Loaded config: {:?}", config);

    let world = Arc::new(World::new());
    let gen = create_world_gen(&config, &world);
    ServerHandler::start(config, world, gen)
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

    Arc::new(GenerationScheduler::new(
        world.clone(),
        Arc::new(WorldGenerator::new(seed, config, world.clone())),
        server_conf.generator_threads,
    ))
}

fn handle_client(
    id: i32,
    in_stream: TcpStream,
    unicast_rx: mpsc::Receiver<Packet>,
    server: Arc<ServerHandler>,
) {
    tokio::spawn(async move {
        let client_addr = in_stream.peer_addr().unwrap();
        debug!("Client {:?} connected", client_addr);

        let codec = MinecraftCodec::new();
        let msg_stream = Framed::new(in_stream, codec);

        let mut handler = ClientHandler::new(id, msg_stream, unicast_rx, server);
        handler.loop_until_disconnect().await;

        debug!("Client {:?} disconnected", client_addr);
    });
}
