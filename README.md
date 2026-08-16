# Rust HTTP/1.1 Server

An async HTTP web server built in rust with tokio.  Supports the following features:
* Reading the files and directories based on the path provided by the user
* Serving a HTML web page to a client on a configurable ip and port.  Hostnames are also accepted.
* Allows client to download files from server.

## Usage

```
Usage: server [OPTIONS]

Options:
      --host <HOST>
  -p, --port <PORT>
  -h, --help         Print help
  -V, --version      Print version
```

## To run
1. Run the following command `cargo run`
1. Open a web browser and navigate to localhost:8080

## Being added
1. Base directory - the option to specify a base directory the user can access on the computer the server is run.  The user is not allowed to access any higher directory in the file tree than the base. Default is /
2. Non UTF-8 files i.e. images - Currently, I have not coded the server in such a way where it can serve non-utf8 content like images and binaries so I need to figure out how I would go about doing that.
