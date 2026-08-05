use server::handle_connection;
use std::io;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> io::Result<()> {
    const URL: &str = "127.0.0.1:8080";
    let listener = TcpListener::bind(&URL).await?;
    println!("Listening on {URL}");

    loop {
        if let Ok((socket, addr)) = listener.accept().await {
            println!("Accepted connection from {addr}");
            tokio::spawn(async move {
                handle_connection(socket).await;
            });
        }
    }
}
