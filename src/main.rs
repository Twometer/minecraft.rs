mod mc;

use log::debug;
use mc::{MinecraftClient, MinecraftServer};
use pretty_env_logger;
use std::{
    io::Result,
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

fn handle_client(stream: TcpStream, server: Arc<Mutex<MinecraftServer>>) {
    debug!("Accepted connection from {}", stream.peer_addr().unwrap());

    let mut client = MinecraftClient::new(stream, server);
    client.receive_loop();
}

fn main() -> Result<()> {
    pretty_env_logger::init();

    let server = MinecraftServer::new();
    let shared_server = Arc::new(Mutex::new(server));

    let listener = TcpListener::bind("127.0.0.1:25565")?;
    for stream in listener.incoming() {
        let stream = stream?;
        let thread_server = shared_server.clone();
        thread::spawn(move || {
            handle_client(stream, thread_server);
        });
    }

    Ok(())
}
