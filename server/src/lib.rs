use handlebars::Handlebars;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::fs::read_to_string;
use tokio::fs::{canonicalize, read_dir};
use tokio::io::{self, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_stream::{StreamExt, wrappers::ReadDirStream};

/// Holds link data
#[derive(Serialize, Deserialize)]
struct Link {
    href: String,      // a link url to be displayed
    file_name: String, // the name of the file or directory to be displayed
    download: String,  // text appended to a link tag if the link represents a file
}

/// Represents files in a directory where the file server is being run
#[derive(Serialize, Deserialize)]
struct FileServer {
    index: String,    // The path from where current files are being displayed
    links: Vec<Link>, // a list of links
}

/// Contains error message information to be displayed when a request cannot be processed
#[derive(Serialize, Deserialize, Debug)]
struct ErrorTemplate {
    error_msg: String,
}

/// Builds an HTTP 1.1 response
/// Returns either a vector of bytes or a generic error type
/// * `name` - The name of the template to be registered
/// * `file_str` - The string handlebars is parsing
/// * `status` - The status code for the http message
fn build_response<T>(
    name: &str,
    file_str: &str,
    status: &str,
    template: &T,
) -> Result<String, std::io::Error>
where
    T: Serialize,
{
    // create a handlebars struct
    let mut handlebars = Handlebars::new();

    // register a template string with the handlebars struct
    if handlebars.register_template_string(name, file_str).is_err() {
        let err_msg = format!("Could not register handlebars template with name {}", name);
        return Err(std::io::Error::new(ErrorKind::InvalidData, err_msg));
    }

    // Attempt to render the template with information
    if let Ok(contents) = handlebars.render(name, template) {
        let length = contents.len();
        return Ok(format_http_response(status, length, contents));
    }

    // throw an error because we could not render the template
    let err_msg = format!("Could not render template file {}", name);
    Err(std::io::Error::new(ErrorKind::InvalidData, err_msg))
}

// HTTP 1.1 status codes
const OK_STATUS: &str = "HTTP/1.1 200 OK";
const ERR_STATUS: &str = "HTTP/1.1 404";

// Relative paths to handlebars template files
const OK_PAGE: &str = "templates/file_tree.hbs";
const ERR_PAGE: &str = "templates/404.hbs";

type MessageHeader = Result<Option<String>, std::io::Error>;

/// Gets the HTTP request header from the request
/// returns either the header as a String or an io error
/// * `header` - the message header to be unwrapped
fn get_header(header: MessageHeader) -> Result<String, std::io::Error> {
    let header_res = header?;
    if let Some(header_res) = header_res {
        return Ok(header_res);
    }

    Err(std::io::Error::new(
        ErrorKind::InvalidInput,
        "Empty http request header",
    ))
}

/// Returns a HTTP/1.1 response message
/// * `status_line` - The type of status code the HTTP/1.1 response message will have
/// * `length` - The size of the HTTP/1.1 message in bytes
/// * `contents` - The body of the HTTP/1.1 message
fn format_http_response(status_line: &str, length: usize, contents: String) -> String {
    format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{contents}")
}

/// Returns a path or an error if the path does not exist
/// * `line` - The path from the http request status line
fn get_path(line: String) -> Result<String, std::io::Error> {
    let re = Regex::new(r"GET (/[a-zA-Z0-9-_.]*)+ HTTP/1.1");
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

        // I do not really know how to test this point because the regex
        // would guarentee that you have to have at least 2 tokens...
        // This is probably dead code but I am going to keep this here unless
        // This happens
        err_msg = format!("HTTP Request type is invalid {} ", line);
        return Err(std::io::Error::new(ErrorKind::InvalidInput, err_msg));
    }

    Err(path_err)
}

/// Does the following:
/// 1. Gets a read and write handle to a socket with an accepted connection
/// 2. Gets the files and directories in the current path
/// 3. Fills in a handlebars template html page with information on files and directories or
///    sends a html page with the error causing the directories and files to not be sent
/// 4. Sends html http 200 response with directories from where server is running if no errors happen,
///    sends html http 404 response with error message, if that fails tries to just send error
///    message, and if all else fails prints the error message to standard error
///    returns an io result
/// * `rx` - read handle to a TCP stream
/// * `tx` - write handle to a TCP stream
async fn serve<Reader, Writer>(rx: &mut Reader, tx: &mut Writer) -> io::Result<()>
where
    Reader: AsyncRead + Unpin,
    Writer: AsyncWrite + Unpin,
{
    let buf_reader = BufReader::new(rx);

    let header = buf_reader.lines().next_line().await;

    let line = get_header(header)?;

    let path = get_path(line)?;

    let cur_path = PathBuf::from(path);

    // check if the path is a valid one.  If its not it is not safe to use this url
    if !cur_path.is_dir() && !cur_path.is_file() {
        return Err(std::io::Error::other("Path does not exist"));
    }

    let mut template = FileServer {
        index: String::from(""),
        links: vec![],
    };

    if cur_path.to_str().is_none() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "Failed to parse path to str",
        ));
    }

    // The path to where the server is running
    template.index = cur_path.to_str().unwrap().to_string();

    // Is the current path a file... send the file data as the response
    if !cur_path.is_dir() && cur_path.is_file() {
        let file_str = read_to_string(cur_path).await?;
        let response_str = format_http_response(OK_STATUS, file_str.len(), file_str);
        return tx.write_all(response_str.as_bytes()).await;
    }

    // read the current directory
    let dirs = read_dir(cur_path).await?;

    // fill the vector with a list of directories and files
    let mut dirs = ReadDirStream::new(dirs);

    // iterate over the current directory.  If the current directory element is a file
    // label the link as "download" which allows you to download the file
    while let Some(dir) = dirs.next().await {
        // if there is an error return it
        if dir.is_err() {
            return Err(dir.err().unwrap());
        }

        // unwrap the result
        let dir = dir.unwrap();
        let mut download = String::from("");
        if dir.file_type().await?.is_file() {
            download = String::from("download");
        }

        // get the path
        let dir = dir.path();
        let dir_err =
            std::io::Error::new(ErrorKind::InvalidData, "Failed to parse file name to str");

        // return error if entry is None
        if dir.file_name().is_none() {
            return Err(dir_err);
        }

        let file_name_opt = dir.file_name().unwrap();

        // throw an error if file name is empty
        if file_name_opt.to_str().is_none() {
            return Err(dir_err);
        }

        // convert to String
        let file_name = file_name_opt.to_str().unwrap().to_string();

        // return the absolute path
        let href = canonicalize(dir).await?;

        // if none returned something went wrong... return an error
        if href.to_str().is_none() {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Failed to parse href to str",
            ));
        }

        // convert the href to a string and put it into the link struct
        let href = href.to_str().unwrap().to_string();

        template.links.push(Link {
            href,
            file_name,
            download,
        });
    }

    // read the file tree template and if that works send that page to the client
    let file_str = read_to_string(OK_PAGE).await?;
    let result = build_response("file_tree", &file_str, OK_STATUS, &template);

    tx.write_all(result?.as_bytes()).await
}

/// Serves the client a page that shows the files on the server at the path where the server is being run
/// * `stream` - TCP socket which data can be read from or sent over
pub async fn handle_connection(stream: TcpStream) {
    let (mut rx, mut tx) = io::split(stream);
    if let Err(err) = serve(&mut rx, &mut tx).await {
        let file_str = read_to_string(ERR_PAGE).await;

        // If we can't even read the 404 not found then we can't serve
        // the user an error
        if let Err(file_parse_err) = file_str {
            eprintln!("{file_parse_err:?}");
            return;
        }

        let file_str = file_str.unwrap();

        let template = ErrorTemplate {
            error_msg: err.to_string(),
        };
        let result = build_response("error_page", &file_str, ERR_STATUS, &template);

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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // tests function that unwraps a HTTP/1.1 request header
    #[test]
    fn get_header_test() {
        // constants
        const HTTP_ERR_MSG: &str = "Empty http request header";
        const ERR_MSG: &str = "Error has occurred";
        const REQUEST_HEADER: &str = "GET / HTTP/1.1";

        // test the working case
        let header: MessageHeader = Ok(Some(String::from(REQUEST_HEADER)));
        let header_result = get_header(header);
        assert!(header_result.is_ok());
        assert_eq!(header_result.unwrap(), String::from(REQUEST_HEADER));

        // passing an err
        let bad_header: MessageHeader = Err(std::io::Error::new(ErrorKind::InvalidInput, ERR_MSG));
        let header_result = get_header(bad_header);
        assert!(header_result.is_err());
        assert_eq!(
            header_result.err().unwrap().to_string(),
            ERR_MSG.to_string()
        );

        // passing an ok none
        let none_header: MessageHeader = Ok(None);
        let header_result = get_header(none_header);
        assert!(header_result.is_err());
        assert_eq!(
            header_result.err().unwrap().to_string(),
            HTTP_ERR_MSG.to_string()
        );
    }

    #[test]
    fn test_format_http() {
        // test that the response function correctly formulates an http response
        const HTTP_RESPONSE: &str =
            "HTTP/1.1 200 OK\r\nContent-Length: 34\r\n\r\nI am a test string of bytes hello!";

        let test_payload = String::from("I am a test string of bytes hello!");
        let response_str = format_http_response(OK_STATUS, test_payload.len(), test_payload);
        assert_eq!(response_str, HTTP_RESPONSE);
    }

    // tests the error case for if the HTTP request header is bad
    fn err_path_tst(header: &str) {
        let path = get_path(String::from(header));
        let err = format!("HTTP Request has invalid regex {}", header);
        assert!(path.is_err());
        assert!(path.as_ref().err().is_some());
        assert_eq!(path.as_ref().err().unwrap().to_string(), err);
    }

    #[test]
    fn test_get_path() {
        // correct
        const REQUEST_HEADER: &str = "GET / HTTP/1.1";
        const REQUEST_HOME: &str = "GET /home HTTP/1.1";
        const EXPECTED_PATH: &str = "/";
        const EXPECTED_HOME_PATH: &str = "/home";

        // unsupported
        const UNSUPPORTED_HEADER: &str = "POST / HTTP/1.1";

        // wrong
        const EMPTY_HEADER_PATH: &str = "GET HTTP/1.1";
        const NON_UTF8_HEADER: &str = "GET /� HTTP/1.1";
        const MISSING_VERSION: &str = "GET /";

        // life good
        let path = get_path(String::from(REQUEST_HEADER));
        assert!(path.is_ok());
        assert_eq!(path.unwrap(), EXPECTED_PATH.to_string());

        // ok
        let path = get_path(String::from(REQUEST_HOME));
        assert!(path.is_ok());
        assert_eq!(path.unwrap(), EXPECTED_HOME_PATH.to_string());

        // bad!
        err_path_tst(UNSUPPORTED_HEADER);

        // bad!
        err_path_tst(EMPTY_HEADER_PATH);

        // bad!!!!
        err_path_tst(NON_UTF8_HEADER);

        // missing version
        err_path_tst(MISSING_VERSION);
    }

    #[tokio::test]
    async fn test_serve() {
        const HTTP_REQUEST: &str = "GET / HTTP/1.1\r\n\
Host: localhost:8080\r\n\
User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:153.0) Gecko/20100101 Firefox/153.0\r\n\
Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8\r\n\
Accept-Language: en-US,en;q=0.9\r\nAccept-Encoding: gzip, deflate, br, zstd\r\n\
Connection: keep-alive\r\n\
Upgrade-Insecure-Requests: 1\r\n\
Sec-Fetch-Dest: document\r\n\
Sec-Fetch-Mode: navigate\r\n\
Sec-Fetch-Site: none\r\n\
Priority: u=0, i\r\n\r\n";
        let mut socket = tokio_test::io::Builder::new()
            .write(HTTP_REQUEST.as_bytes())
            .build();

        let (mut rx, mut tx) = io::split(socket);

        // TODO: Re-write function so you can mock out all of the I/O. i.e. the file system io
        let _ = serve(&mut rx, &mut tx).await;
        println!("After");

        let buf: Vec<u8> = vec![];

        let buf_reader = BufReader::new(rx);

        let my_str = str::from_utf8(buf_reader.buffer());
    }
}
