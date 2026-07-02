use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncWriteExt, BufReader, AsyncBufReadExt, Error};
use tokio::fs::{read_to_string, read_dir};
use tokio_stream::{StreamExt, wrappers::ReadDirStream};
use tokio::time::{sleep, Duration};
use sailfish::TemplateSimple;


#[derive(TemplateSimple)]
#[template(path = "file_tree.stpl")]
struct FileTreeTemplate {
    index: String,
    links: Vec<String>
}

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

async fn handle_connection(stream: TcpStream) {
    let (mut rx, mut tx) = io::split(stream);
    let buf_reader = BufReader::new(&mut rx);

    let header = buf_reader.lines().next_line().await;
    let line = get_header(header);
    
    let (status_line, filename) = match &line[..] {
        "GET / HTTP/1.1" => ("HTTP/1.1 200 OK", "test.html"),
        "GET /sleep HTTP/1.1" => {
            sleep(Duration::from_secs(20)).await; 
            println!("20 seconds elasped");
            ("HTTP/1.1 200 OK", "test.html")
        }
        _ => ("HTTP/1.1 404 NOT FOUND", "404.html")
    };

    let dirs = read_dir(".").await;
    let mut links: Vec<String> = Vec::new();
    if let Ok(dirs) = dirs {
            let mut dirs = ReadDirStream::new(dirs);
            while let Some(dir) = dirs.next().await {
                if let Ok(dir) = dir {
                    let raw_dir = dir.path().display().to_string();
                    let start = 2;
                    links.push(raw_dir[start..].to_string());
                }
            }
    } 
    
    // TODO: Make index update when you click a link
    // TODO: Save and update current directory for session
    // TODO: do basic input validation on url
    // TODO: server icons
    // TODO: clean up the code 
    // TODO: DONE!
    let template = FileTreeTemplate { 
        index: String::from("/"),
        links 
    };
    let contents = template.render_once().unwrap();
    let length = contents.len();
    let response = 
        format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");
    let bytes = response.as_bytes();

    match tx.write_all(bytes).await {
        Ok(response) => {
            if response != () {
               println!("{response:?}"); 
            }
        },
        Err(err) => {
            eprintln!("{err:?}");
        },
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
