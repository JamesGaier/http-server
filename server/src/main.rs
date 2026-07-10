use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncWriteExt, BufReader, AsyncBufReadExt, Error};
use tokio::fs::{read_dir, metadata, canonicalize};
use tokio_stream::{StreamExt, wrappers::ReadDirStream};
use std::env::current_dir;
use std::path::PathBuf;
use regex::Regex;
use std::collections::HashMap;
use handlebars::Handlebars;
//use serde_json::{json, Serialize, Deserialize};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Link {
    href: String,
    file_name: String,
}

#[derive(Serialize, Deserialize)]
struct Template {
    index: String,
    links: Vec<Link>
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

    //let combined = dir.clone() + &path;
    let mut cur_path = PathBuf::from(dir.clone());
    cur_path.push(&path);
    
    // change the directory the process is looking at to this one
    if !cur_path.is_dir() && !cur_path.is_file() {
        return
    }

    let mut template = Template {
        index: String::from(""),
        links: vec![]
    };
    template.index = String::from(cur_path.to_str().unwrap());
    let dirs = read_dir(cur_path).await;
    // fill the vector with a list of directories and files
    if let Ok(dirs) = dirs {
            let mut dirs = ReadDirStream::new(dirs);
            while let Some(dir) = dirs.next().await {
                if let Ok(dir) = dir {
                    let dir = dir.path();
                    let tst = dir.file_name().unwrap().to_str().unwrap().to_string();
                    let tst1 = canonicalize(dir).await.unwrap();

                    template.links.push(Link{
                        href: tst1.to_str().unwrap().to_string(),
                        file_name: tst,
                    });
                }
            }
    } 

    
    // TODO: Save and update current directory for session
    // TODO: do basic input validation on url
    // TODO: server icons
    // TODO: Serve 404 page if all resources are not able to be fetched and display error
    // TODO: clean up the code 
    // TODO: DONE!
    
    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_file("file_tree", "templates/file_tree.hbs");
    
    // populate template with fields
    //let contents = template.render_once().unwrap();
    let contents = handlebars.render("file_tree", &template).unwrap();
    println!("{contents:?}");

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
