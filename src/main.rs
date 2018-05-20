extern crate num_cpus;
extern crate rust_hello;

use rust_hello::ThreadPool;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;
use std::path::Path;
use std::process;

fn handle_connection(mut stream: TcpStream, files: Box<HashMap<String, String>>) {
    let mut buffer = [0; 512];
    stream.read(&mut buffer).unwrap();

    let mut status_code: u16 = 404;
    let mut status_message = String::from("Not Found");
    let mut contents = String::from("404 - Not Found");

    if let Some(x) = files.get("/index.html") {
        contents = x.clone();
        status_code = 200;
        status_message = String::from("OK");
    }

    let response = format!(
        "HTTP/1.1 {} {}\n\n{}\n",
        status_code, status_message, contents
    );

    stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();
    let pool = ThreadPool::new(num_cpus::get() << 10);
    let mut files = Box::new(HashMap::new());

    if Path::new("index.html").exists() {
        let mut contents = String::new();
        File::open("index.html")
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        files.insert(String::from("/index.html"), contents);
    }

    for stream in listener.incoming() {
        let stream = stream.unwrap();
        let files = files.clone();
        pool.execute(|| {
            handle_connection(stream, files);
        });
    }
    process::exit(0);
}
