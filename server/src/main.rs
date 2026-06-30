use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, BufReader, AsyncBufReadExt, Error, ReadHalf, Lines};
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;

type MessageHeader = Result<Option<String>, Error>;
fn get_header(header: MessageHeader) -> String {
    match header {
        Ok(opt_line) => {
            match opt_line {
                Some(line) => {
                    line
                },
                None => {
                    eprintln!("Line from http request is empty");
                    String::new()
                },
            }
        },
        Err(err) => {
            eprintln!("{err:?}");
            String::new()
        },
    }
}

async fn handle_connection(mut stream: TcpStream) {
    let (mut rx, mut tx) = io::split(stream);
    let buf_reader = BufReader::new(&mut rx);

    let header = buf_reader.lines().next_line().await;
    let line = get_header(header);
    
    // TODO: Reject if any other type of request other than GET or HEAD


    // TODO: Run ls command 
    
    // TODO: Return files from current directory
}

#[tokio::main]
async fn main() {
    
    let url = "127.0.0.1:8080";

    let listener = loop {
        println!("Creating listener on port {url}");
        let listener_res = TcpListener::bind(&url).await;

        match listener_res {
            Ok(listener) => {
                break listener 
            },
            Err(err) => {
                eprintln!("{err:#?}");
            },
        }
    };
    
    
    loop {
        let socket_res = listener.accept().await;
        let socket = match socket_res {
            Ok((socket, addr)) => {
                println!("Accepted connection from {addr}");
                socket
            },
            Err(err) => {
                eprintln!("{err:#?}");  
                continue
            },
        };
        
        tokio::spawn(async move {
            handle_connection(socket).await;
        });
    }

}
