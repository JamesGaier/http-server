use clap::Parser;
use server::handle_connection;
use std::io;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// Ip and port data
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long, value_name = "HOST")]
    host: Option<String>, // Either a hostname or an ip address

    #[arg(short, long, value_name = "PORT")]
    port: Option<u16>, // a valid port number
}

// TODO: Add unit tests for the server module: Partly done... need to take out the two biggest
// functions
// TODO: Add option to log to a file
// TODO: Add support for "base dir" option that limits the user to only child directories of the
// base
#[tokio::main]
async fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let mut server_host = String::from("127.0.0.1");
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

    // create a oneshot channel... we are only ever gunna hit CTRL+C once
    let (resp_tx, resp_rx) = oneshot::channel();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        resp_tx.send(true).unwrap();
    });

    // do whichever is not blocking first
    // i.e. if we do not get a message saying CTRL+C was pressed we can have a server running
    // the first arm is a channel which will block until a message is received
    // the second arm is a future which will block forever
    tokio::select! {
        _ = resp_rx => {
            eprintln!("CTRL+C pressed!");
        },
        _ = async {
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
        } => {}
    }

    Ok(())
}
