use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncWriteExt, BufReader, AsyncBufReadExt, Error};
use tokio::fs::{read_dir, metadata};
use tokio_stream::{StreamExt, wrappers::ReadDirStream};
use sailfish::TemplateSimple;
use std::env::current_dir;
use regex::Regex;


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
    
    let status_line = "HTTP/1.1 200 OK";

    // check that the path is a valid path
    let re = Regex::new(r"GET (/[a-zA-Z0-9-_.]+)+ HTTP/1.1").unwrap();
    let mut path: String = String::from("");
    if re.is_match(&line) {

        // get the path from the GET request
        let tokens = line.split_whitespace().collect::<Vec<&str>>(); 
        if tokens.len() > 2 {
           path = String::from(tokens[1]);  
        }
    }

    // switch to using rusts Path object

    // TODO: cd to the directory from the path if the path is a directory and not a file
    // get the current path

    // I am a little iffy on this... I do not see an async way to do this
    // so I am using the env function I would assume this doesn't block
    // so its probably okay??
    let dir = match current_dir() {
        Ok(dir) => {
            match dir.into_os_string().into_string() {
                Ok(dir) => {
                    dir
                },
                Err(err) => {
                    eprintln!("{err:?}");
                    String::new()
                }
            }
        },
        Err(err) => {
            eprintln!("{err:?}");
            String::new()
        }
    };

    let combined = dir.clone() + &path;
    
    let mut valid_dir = false;
    let mut valid_file = false;
    match metadata(combined.clone()).await {
        Ok(meta) => {
            valid_dir = meta.file_type().is_dir();
            valid_file = meta.file_type().is_file();
        },
        Err(err) => {
            // TODO: Add a redirect to the 404 page
            println!(" {combined:?} {err:?}");
        },
    };


    // change the directory the process is looking at to this one
    
    if !valid_dir && !valid_file {
        return
    }

    let dirs = read_dir(combined.clone()).await;
    // fill the vector with a list of directories and files
    let mut links: Vec<String> = Vec::new();
    if let Ok(dirs) = dirs {
            let mut dirs = ReadDirStream::new(dirs);
            while let Some(dir) = dirs.next().await {
                if let Ok(dir) = dir {
                    let raw_dir = dir.path().display().to_string();
                    //let start = 2;
                    //links.push(raw_dir[start..].to_string());
                    links.push(raw_dir[0..].to_string());
                }
            }
    } 
    
    // TODO: Make index update when you click a link
    // TODO: Save and update current directory for session
    // TODO: do basic input validation on url
    // TODO: server icons
    // TODO: Serve 404 page if all resources are not able to be fetched and display error
    // TODO: clean up the code 
    // TODO: DONE!
    
    
    // setup templated html
    let template = FileTreeTemplate { 
        index: dir,
        links 
    };

    // populate template with fields
    let contents = template.render_once().unwrap();

    // format http response
    let length = contents.len();
    let response = 
        format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}");

    // serialize
    let bytes = response.as_bytes();

    // write response and handle errors
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
