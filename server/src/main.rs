use std::io::ErrorKind;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{self, AsyncWriteExt, BufReader, AsyncBufReadExt, AsyncRead, AsyncWrite, ReadHalf, WriteHalf};
use tokio::fs::{read_dir, canonicalize};
use tokio_stream::{StreamExt, wrappers::ReadDirStream};
use std::env::current_dir;
use regex::Regex;
use handlebars::Handlebars;
use serde::{Serialize, Deserialize};
use std::sync::Mutex;
use tokio::fs::read_to_string;

#[derive(Serialize, Deserialize)]
struct Link {
    href: String,
    file_name: String,
    download: String,
}


#[derive(Serialize, Deserialize)]
struct FileServer {
    index: String,
    links: Vec<Link>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ErrorTemplate {
    error_msg: String
}


/// Builds an HTTP 1.1 response
/// Returns either a vector of bytes or a generic error type 
/// * `name` - The name of the template to be registered
/// * `file_str` - The string handlebars is parsing
/// * `status` - The status code for the http message
fn build_response<T>(name: &str, file_str: &str, status: &str, template: &T) -> Result<String, std::io::Error>
where
T: Serialize
{
    let mut handlebars = Handlebars::new();
    if handlebars.register_template_string(name, file_str).is_err() {
        let err_msg = format!("Could not register handlebars template with name {}", name);
        return Err(std::io::Error::new(ErrorKind::InvalidData, err_msg));
    }

    if let Ok(contents) = handlebars.render(&name, template) {
        let length = contents.len();
        return Ok(format_http_response(status, length, contents));
    }


    let err_msg = format!("Could not render template file {}", name);
    return Err(std::io::Error::new(ErrorKind::InvalidData, err_msg));
}



const OK_STATUS: &str = "HTTP/1.1 200 OK";
const ERR_STATUS: &str = "HTTP/1.1 404";
const OK_PAGE: &str = "templates/file_tree.hbs";
const ERR_PAGE: &str = "templates/404.hbs";

type MessageHeader = Result<Option<String>, std::io::Error>;
fn get_header(header: MessageHeader) -> Result<String, std::io::Error> {
    let header_res = header?;
    if let Some(header_res) = header_res {
        return Ok(header_res);
    }

    return Err(std::io::Error::new(ErrorKind::InvalidInput, "Empty http request header"));
}


fn format_http_response(status_line: &str, length: usize, contents: String) -> String {
    format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}")
}

fn get_path(line: String) -> Result<String, std::io::Error> {
    let re = Regex::new(r"GET (/[a-zA-Z0-9-_.]+)+ HTTP/1.1");
    let mut err_msg = format!("HTTP Request has invalid regex {}", line);
    let path_err = std::io::Error::new(ErrorKind::InvalidInput, err_msg);

    if let Err(re_err) = re {
        eprintln!("{}", re_err);
        return Err(path_err);
    }

    let re = re.unwrap();

    if re.is_match(&line) {
        // get the path from the GET request
        let tokens = line.split_whitespace().collect::<Vec<&str>>(); 
        if tokens.len() > 2 {
           return Ok(String::from(tokens[1]));
        }
        
        err_msg = format!("HTTP Request type is invalid {} ", line);
        return Err(std::io::Error::new(ErrorKind::InvalidInput, err_msg));
    }

    return Ok(String::new());
}


async fn serve<T: AsyncRead + AsyncWrite>(rx: &mut ReadHalf<T>, tx: &mut WriteHalf<T>) -> io::Result<()> {
    let buf_reader = BufReader::new(rx);

    let header = buf_reader
        .lines()
        .next_line().await;

    let line = get_header(header)?;
    
    let path = get_path(line)?;

    
    // using sync io check that this is okay?
    let mut cur_path = current_dir()?;
    cur_path.push(&path);
    
    // check if the path is a valid one.  If its not it is not safe to use this url
    if !cur_path.is_dir() && !cur_path.is_file() {
        return Err(std::io::Error::new(ErrorKind::Other, "Path does not exist"));
    }

    let mut template = FileServer {
        index: String::from(""),
        links: vec![],
    };

    if cur_path.to_str().is_none() {
        return Err(std::io::Error::new(ErrorKind::InvalidData, "Failed to parse path to str"));
    }

    template.index = cur_path
        .to_str()
        .unwrap()
        .to_string();

    if !cur_path.is_dir() && cur_path.is_file() {
        let file_str = read_to_string(cur_path).await?;
        let response_str = format_http_response(OK_STATUS, file_str.len(), file_str);
        return Ok(tx.write_all(response_str.as_bytes()).await?);
    }

    let dirs = read_dir(cur_path).await?;

    // fill the vector with a list of directories and files
    let mut dirs = ReadDirStream::new(dirs);
    while let Some(dir) = dirs.next().await {
        if let Ok(dir) = dir {
            let mut download_text = String::from("");
            if dir.file_type().await?.is_file() {
                println!("is file");
                download_text = String::from("download");
            }

            let dir = dir.path();

            let dir_err = std::io::Error::new(ErrorKind::InvalidData, "Failed to parse file name to str");

            if dir.file_name().is_none() {
                return Err(dir_err);
            }

            let file_name_opt = dir
                .file_name()
                .unwrap();

            if file_name_opt.to_str().is_none() {
                return Err(dir_err);
            }

            let file_name = file_name_opt
                .to_str()
                .unwrap()
                .to_string();

            let href = canonicalize(dir).await?;

            if href.to_str().is_none() {
                return Err(std::io::Error::new(ErrorKind::InvalidData, "Failed to parse href to str"));
            }

            let href = href
                .to_str()
                .unwrap()
                .to_string();

            template.links.push(Link{
                href: href,
                file_name: file_name,
                download: download_text
            });
        }
    }
    
    let file_str = read_to_string(OK_PAGE).await?;
    let result = build_response(
        "file_tree",
        &file_str,
        OK_STATUS,
        &template
    );

    return Ok(tx.write_all(result?.as_bytes()).await?);
}

async fn handle_connection(stream: TcpStream) {
    let (mut rx, mut tx) = io::split(stream);
    if let Err(err) =  serve::<tokio::net::TcpStream>(&mut rx, &mut tx).await {
        
        let file_str = read_to_string(ERR_PAGE).await;

        // If we can't even read the 404 not found then we can't serve
        // the user an error
        if let Err(file_parse_err) = file_str {
            eprintln!("{file_parse_err:?}");
            return;  
        }

        let file_str  = file_str.unwrap();

        let template = ErrorTemplate {
            error_msg: err.to_string()
        };
        let result = build_response(
            "error_page",
            &file_str,
            ERR_STATUS,
            &template
        );

        // not much we can do here either... maybe just write the error no template as a last 
        // resort
        if let Err(err) = result {
            if let Err(err) = tx.write_all(err.to_string().as_bytes()).await {
                eprintln!("{err:?}");
                return;
            }

            eprintln!("{err:?}");
            return;
        }

        let result = result.unwrap();

        // shrug not much we can do if the socket won't write
        if let Err(err) = tx.write_all(result.as_bytes()).await {
            eprintln!("{err:?}");
            return;
        }
    }
}

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
