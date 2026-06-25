use tokio::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::ops::DerefMut;


async fn handle_connection(_socket: TcpStream, con_num: Arc<Mutex<i32>>) {
    {
        let mut con_num = con_num.lock().unwrap();
        println!("Accepted connection {con_num}");
        *con_num += 1;
    }
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
    
    
    let con_num = Arc::new(Mutex::new(0));
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
        
        let con_num = con_num.clone();
        tokio::spawn(async move {
            handle_connection(socket, con_num).await;
        });
    }

}
