use handlebars::Handlebars;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::fs::{canonicalize, read_dir};
use tokio::fs::{metadata, read_to_string};
use tokio::io::{self, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio_stream::{StreamExt, wrappers::ReadDirStream};
//use std::error::Error;
use std::path::Path;
// compiler things this is unused even though its used in my tokio_test lol
#[allow(unused)]
use std::ffi::OsStr;

/// Holds link data
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Link {
    href: String,      // a link url to be displayed
    file_name: String, // the name of the file or directory to be displayed
    download: String,  // text appended to a link tag if the link represents a file
}

/// Represents files in a directory where the file server is being run
#[derive(Serialize, Deserialize, Debug)]
struct FileServer {
    index: String,    // The path from where current files are being displayed
    links: Vec<Link>, // a list of links
}

/// Contains error message information to be displayed when a request cannot be processed
#[derive(Serialize, Deserialize, Debug)]
struct ErrorTemplate {
    error_msg: String,
}

trait IFilesystemIO {
    async fn read_to_string(&self, path: impl AsRef<Path>) -> io::Result<String>;
    async fn canonicalize(&self, dir: impl AsRef<Path>) -> io::Result<PathBuf>;
    async fn read_dir(&self, dir: impl AsRef<Path>) -> io::Result<Vec<Link>>;
    async fn is_dir(&self, dir: impl AsRef<Path>) -> io::Result<bool>;
    async fn is_file(&self, dir: impl AsRef<Path>) -> io::Result<bool>;
}

struct AsyncIOImpl {}

// this code is used... maybe the compiler just doesn't see it because its a tokio_test
#[allow(unused)]
struct MockIOImpl {
    fake_file_output: String,
    fake_abs_path: PathBuf,
    fake_dirs: Vec<Link>,
    fake_base_file: bool,
    fake_base_dir: bool,
    is_metadata_dir_err: bool,
    is_metadata_file_err: bool,
    is_read_to_string_err: bool,
    is_canonicalize_err: bool,
    is_read_dir_err: bool,
}

impl MockIOImpl {
    // this code is used... maybe the compiler just doesn't see it because its a tokio_test
    #[allow(unused)]
    fn build(
        fake_file_output: String,
        fake_abs_path: PathBuf,
        fake_dirs: Vec<Link>,
        fake_base_file: bool,
        fake_base_dir: bool,
    ) -> MockIOImpl {
        MockIOImpl {
            fake_file_output,
            fake_abs_path,
            fake_dirs,
            fake_base_file,
            fake_base_dir,
            is_metadata_dir_err: false,
            is_metadata_file_err: false,
            is_read_to_string_err: false,
            is_canonicalize_err: false,
            is_read_dir_err: false,
        }
    }
}

impl IFilesystemIO for AsyncIOImpl {
    async fn read_to_string(&self, path: impl AsRef<Path>) -> io::Result<String> {
        read_to_string(path).await
    }

    async fn canonicalize(&self, dir: impl AsRef<Path>) -> io::Result<PathBuf> {
        canonicalize(dir).await
    }

    async fn read_dir(&self, cur_path: impl AsRef<Path>) -> io::Result<Vec<Link>> {
        let mut to_ret = vec![];
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
            let href = self.canonicalize(dir).await?;

            // if none returned something went wrong... return an error
            if href.to_str().is_none() {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    "Failed to parse href to str",
                ));
            }

            // convert the href to a string and put it into the link struct
            let href = href.to_str().unwrap().to_string();

            to_ret.push(Link {
                href,
                file_name,
                download,
            });
        }

        Ok(to_ret)
    }

    async fn is_dir(&self, dir: impl AsRef<Path>) -> io::Result<bool> {
        Ok(metadata(dir).await?.is_dir())
    }

    async fn is_file(&self, dir: impl AsRef<Path>) -> io::Result<bool> {
        Ok(metadata(dir).await?.is_file())
    }
}

impl IFilesystemIO for MockIOImpl {
    async fn read_to_string(&self, _path: impl AsRef<Path>) -> io::Result<String> {
        if self.is_read_to_string_err {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Could not read file to string",
            ));
        }

        Ok(self.fake_file_output.clone())
    }

    async fn canonicalize(&self, _dir: impl AsRef<Path>) -> io::Result<PathBuf> {
        if self.is_canonicalize_err {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Could not get absolute path",
            ));
        }

        Ok(self.fake_abs_path.clone())
    }

    async fn read_dir(&self, dir: impl AsRef<Path>) -> io::Result<Vec<Link>> {
        // I put this here because the real read_dir function has a canonicalize call
        self.canonicalize(dir).await?;

        if self.is_read_dir_err {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Could not read directory tree",
            ));
        }

        Ok(self.fake_dirs.clone())
    }

    async fn is_dir(&self, _dir: impl AsRef<Path>) -> io::Result<bool> {
        if self.is_metadata_dir_err {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Could not get metadata on dir",
            ));
        }

        Ok(self.fake_base_dir)
    }

    async fn is_file(&self, _dir: impl AsRef<Path>) -> io::Result<bool> {
        if self.is_metadata_file_err {
            return Err(std::io::Error::new(
                ErrorKind::InvalidData,
                "Could not get metadata on file",
            ));
        }

        Ok(self.fake_base_file)
    }
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

    let reg_res = handlebars.register_template_string(name, file_str);
    // register a template string with the handlebars struct
    if reg_res.is_err() {
        let err_msg = reg_res.err().unwrap().reason().to_string();

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
/// * `io` - io interface which allows DI so we can mock out actual IO with fake io
async fn serve<Reader, Writer, IO>(rx: Reader, mut tx: Writer, io: &IO) -> io::Result<()>
where
    Reader: AsyncRead + Unpin,
    Writer: AsyncWrite + Unpin,
    IO: IFilesystemIO,
{
    let buf_reader = BufReader::new(rx);
    let header = buf_reader.lines().next_line().await;

    let line = get_header(header)?;

    // check if the path is a valid one.  If its not it is not safe to use this url
    let path = get_path(line)?;

    let cur_path = PathBuf::from(path);

    // am I a dir or a file
    let is_dir = io.is_dir(&cur_path).await?;
    let is_file = io.is_file(&cur_path).await?;

    if !is_dir && !is_file {
        return Err(std::io::Error::other("Path does not exist"));
    }

    let mut template = FileServer {
        index: String::from(""),
        links: vec![],
    };

    // I do not know how to test here given a regex prevents empty paths...
    // It may be a place the code can never go... oh well better safe than sorry
    if cur_path.to_str().is_none() {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "Failed to parse path to str",
        ));
    }

    // The path to where the server is running
    template.index = cur_path.to_str().unwrap().to_string();

    // Is the current path a file... send the file data as the response
    if !is_dir && is_file {
        let file_str = io.read_to_string(cur_path).await?;
        let response_str = format_http_response(OK_STATUS, file_str.len(), file_str);
        return tx.write_all(response_str.as_bytes()).await;
    }

    let mut links = io.read_dir(cur_path).await?;
    template.links.append(&mut links);

    // read the file tree template and if that works send that page to the client
    let file_str = io.read_to_string(OK_PAGE).await?;
    let result = build_response("file_tree", &file_str, OK_STATUS, &template)?;

    tx.write_all(result.as_bytes()).await
}

/// Serves the client a page that shows the files on the server at the path where the server is being run
/// * `stream` - TCP socket which data can be read from or sent over
pub async fn handle_connection(stream: TcpStream) {
    let (mut rx, mut tx) = io::split(stream);
    let io = AsyncIOImpl {};
    if let Err(err) = serve(&mut rx, &mut tx, &io).await {
        let file_str = io.read_to_string(ERR_PAGE).await;

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

    async fn check_readonly_err(mock_io: &MockIOImpl, http_request: &String, serve_err: &String) {
        let mut rx = tokio_test::io::Builder::new()
            .read(http_request.as_bytes())
            .build();

        let mut tx = tokio_test::io::Builder::new().build();
        let path_err = serve(&mut rx, &mut tx, mock_io).await.err();
        assert!(path_err.is_some());
        assert_eq!(path_err.unwrap().to_string(), *serve_err);
    }

    #[tokio::test]
    async fn test_serve() {
        // Section one... Setup the mock object
        let ok_template = read_to_string(OK_PAGE).await.unwrap();
        let fake_abs_path: PathBuf = PathBuf::from(OsStr::new("/home/jamesgaier/Desktop"));
        let fake_dirs: Vec<Link> = vec![
            Link {
                href: "/home/jamesgaier/Desktop/Fluent-gtk-theme".to_string(),
                file_name: "Fluent-gtk-theme".to_string(),
                download: "".to_string(),
            },
            Link {
                href: "/home/jamesgaier/Desktop/FramePack".to_string(),
                file_name: "FramePack".to_string(),
                download: "".to_string(),
            },
            Link {
                href: "/home/jamesgaier/Desktop/Fooocus".to_string(),
                file_name: "Fooocus".to_string(),
                download: "".to_string(),
            },
        ];

        let mock_io = MockIOImpl::build(
            ok_template.to_string(), // the template
            fake_abs_path.clone(),   // the abs path
            fake_dirs.clone(),       // the links
            false,                   // am i a file
            true,                    // am i a dir
        );

        // get a test message that I created this should be what the server outputs
        let http_response = read_to_string("test_messages/page.html").await.unwrap();

        // get a request from the browser
        let http_request = read_to_string("test_messages/http_req.html").await.unwrap();

        // allows me to pass fake data into the read side of the socket and
        // verify that the write side's output matches what I put into the write function
        let mut rx = tokio_test::io::Builder::new()
            .read(http_request.as_bytes())
            .build();

        let mut tx = tokio_test::io::Builder::new()
            .write(http_response.as_bytes())
            .build();

        // we do not care what this returns because the mock classes handle verifying the read and
        // write is correct
        let _ = serve(&mut rx, &mut tx, &mock_io).await.unwrap();

        // test the case where the metadata is messed up
        let mut mock_io = MockIOImpl::build(
            ok_template.to_string(), // the template
            fake_abs_path.clone(),   // the abs path
            fake_dirs.clone(),       // the links
            false,                   // am i a file
            false,                   // am i a dir
        );

        // everything is okay but the file is all messed up
        // its neither a file nor a dir
        let err_msg = String::from("Path does not exist");
        check_readonly_err(&mock_io, &http_request, &err_msg).await;

        // now we are a file
        mock_io.fake_base_file = true;
        mock_io.fake_base_dir = false;

        // the dir metadata is messed up
        mock_io.is_metadata_dir_err = true;
        let err_msg = String::from("Could not get metadata on dir");
        check_readonly_err(&mock_io, &http_request, &err_msg).await;

        // file metadata has error
        mock_io.is_metadata_dir_err = false;
        mock_io.is_metadata_file_err = true;
        let err_msg = String::from("Could not get metadata on file");
        check_readonly_err(&mock_io, &http_request, &err_msg).await;

        // reading the handlebars template gets messed up
        mock_io.is_metadata_file_err = false;
        mock_io.is_read_to_string_err = true;
        let err_msg = String::from("Could not read file to string");
        check_readonly_err(&mock_io, &http_request, &err_msg).await;

        // we only hit the absolute path code if its a dir
        mock_io.fake_base_file = false;
        mock_io.fake_base_dir = true;

        mock_io.is_read_to_string_err = false;
        mock_io.is_canonicalize_err = true;
        let err_msg = String::from("Could not get absolute path");
        check_readonly_err(&mock_io, &http_request, &err_msg).await;

        mock_io.is_canonicalize_err = false;
        mock_io.is_read_dir_err = true;
        let err_msg = String::from("Could not read directory tree");
        check_readonly_err(&mock_io, &http_request, &err_msg).await;
        mock_io.is_read_dir_err = false;

        // test handlebars errors
        // junk
        mock_io.fake_file_output = "<html>{{link}</ht".to_string();
        let err_msg = String::from(
            "invalid handlebars syntax: expected identifier, helper_parameter, or trailing_tilde_to_omit_whitespace",
        );
        check_readonly_err(&mock_io, &http_request, &err_msg).await;
    }
}
