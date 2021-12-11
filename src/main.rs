mod mc;

use log::{debug, error, info};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};

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

fn handle_client(mut stream: TcpStream) {
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];

        loop {
            let n = match stream.read(&mut buf).await {
                Ok(n) if n == 0 => return,
                Ok(n) => n,
                Err(e) => {
                    error!("Client disconnected: {}", e);
                    return;
                }
            };

            debug!("Read {} bytes", n);
        }
    });
}
