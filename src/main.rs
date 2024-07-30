use std::{io::Error, net::TcpListener};
use zero2prod::run;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let listener = TcpListener::bind("127.0.0.1:8000").expect("Failed to bind random port");
    run(listener)?.await
}
