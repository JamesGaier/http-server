use clap::Parser;
use server::handle_connection;
use std::io;
use tokio::net::TcpListener;

// Ip and port data
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, value_name = "HOST")]
    host: Option<String>, // Either a hostname or an ip address

    #[arg(short, long, value_name = "PORT")]
    port: Option<u16>,    // a valid port number
}

// TODO: Add unit tests for the server module
#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let mut server_host = String::from("localhost");

    if let Some(host) = cli.host {
        // use host validator to make sure host is valid
        if hostname_validator::is_valid(&host) {
            // set option if host is valid
            server_host = host;
        }
    }

    let mut server_port: u16 = 8080;
    if let Some(port) = cli.port {
        server_port = port;
    }

    // listen to server on configured port
    let url: String = format!("{}:{}", server_host, server_port);
    let listener = TcpListener::bind(&url).await?;
    println!("Listening on {url}");

    loop {
        // if you can accept a connection accept it
        if let Ok((socket, addr)) = listener.accept().await {
            println!("Accepted connection from {addr}");
            // pass the task off to another thread
            tokio::spawn(async move {
                handle_connection(socket).await;
            });
        }
    }
}
