mod client;
mod mc;

use crate::client::ClientHandler;
use crate::mc::codec::MinecraftCodec;
use log::{debug, info};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::Framed;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    pretty_env_logger::init();
    info!("Starting server...");

    let listener = TcpListener::bind("127.0.0.1:25565").await?;
    info!("Listener bound and ready");

    loop {
        let (stream, _) = listener.accept().await?;
        handle_client(stream);
    }
}

fn handle_client(stream: TcpStream) {
    tokio::spawn(async move {
        let client_addr = stream.peer_addr().unwrap();
        debug!("Client {:?} connected", client_addr);

        let codec = MinecraftCodec::new();
        let framed = Framed::new(stream, codec);

        let mut handler = ClientHandler::new(framed);
        handler.handler_loop().await;

        debug!("Client {:?} disconnected", client_addr);
    });
}
