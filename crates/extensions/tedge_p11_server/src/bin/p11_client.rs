use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

fn main() {
    // Define the path for the UNIX socket
    let socket_path = "/tmp/rust_unix_socket";

    // Connect to the UNIX socket
    let mut stream = UnixStream::connect(socket_path).unwrap();
    println!("Connected to the server");

    // Send data to the server
    stream.write_all(b"Hello from client").unwrap();

    // Read the response from the server
    let mut buffer = [0; 1024];
    match stream.read(&mut buffer) {
        Ok(n) => println!("Received data: {}", String::from_utf8_lossy(&buffer[..n])),
        Err(e) => eprintln!("Failed to read from socket: {}", e),
    }
}
