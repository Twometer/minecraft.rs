mod mc;

use log::debug;
use mc::{MinecraftClient, MinecraftServer};
use pretty_env_logger;
use std::{
    io::Result,
    net::{TcpListener, TcpStream},
    thread,
};

fn handle_client(stream: TcpStream) {
    debug!("Accepted connection from {}", stream.peer_addr().unwrap());

    let mut client = MinecraftClient::new(stream);
    client.receive_loop();
}

fn main() -> Result<()> {
    pretty_env_logger::init();

    let mut server = MinecraftServer::new();
    let listener = TcpListener::bind("127.0.0.1:25565")?;
    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(|| {
            handle_client(stream);
        });
    }

    Ok(())
}
