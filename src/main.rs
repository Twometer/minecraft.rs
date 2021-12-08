mod mc;

use log::debug;
use mc::MinecraftConnection;
use pretty_env_logger;
use std::{
    io::Result,
    net::{TcpListener, TcpStream},
    thread,
};

fn handle_client(stream: TcpStream) {
    debug!("Accepted connection from {}", stream.peer_addr().unwrap());

    MinecraftConnection::new(stream).receive_loop();
}

fn main() -> Result<()> {
    pretty_env_logger::init();

    let listener = TcpListener::bind("127.0.0.1:25565")?;

    for stream in listener.incoming() {
        let stream = stream?;
        thread::spawn(|| {
            handle_client(stream);
        });
    }
    Ok(())
}
