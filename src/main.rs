extern crate chrono;
extern crate num_cpus;
extern crate rust_hello;

use chrono::prelude::*;
use rust_hello::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process;
// use std::time::Duration;

fn handle_connection(mut stream: TcpStream, files: HashMap<String, String>) {
    let mut bytes_read: usize = 0;
    let mut request_raw = Box::new(String::new());

    let mut status_code: u16 = 404;
    let mut status_message = String::from("Not Found");
    let headers: Box<String>;
    let mut contents = Box::new(String::from("404 - Not Found"));

    // TODO: define datatransfer watchdog and reject connection when watchdog is not reset
    // stream.set_read_timeout(Some(Duration::new(10, 0))).unwrap();
    // stream.set_write_timeout(Some(Duration::new(10, 0))).unwrap();

    // TODO: read up to upper bound and reject connection when reached
    {
        let mut buffer = [0; 1024];
        while bytes_read < 50_000_000 && stream.read(&mut buffer).unwrap() <= 1024 {
            let buffer_raw = String::from_utf8_lossy(&buffer[..]);
            let is_last_buffer = buffer_raw.ends_with("\0");
            let buffer_raw = string_trim_end(&buffer_raw);
            bytes_read += buffer_raw.len();
            request_raw.push_str(buffer_raw);
            if is_last_buffer {
                break;
            }
        }
    }

    if bytes_read >= 50_000_000 {
        status_code = 413;
        status_message = String::from("Payload Too Large");
        contents = Box::new(String::from("413 - Payload Too Large"));
        headers = Box::new(format!(
            "Content-Length: {}\r\nConnection: Keep-Alive\r\nDate: {}\r\nServer: rust\r\nContent-Type: text/html; charset=utf-8",
            contents.len() + 2,
            Utc::now().format("%a, %b %e %Y %T GMT").to_string()
        ));
        let response = format!(
            "HTTP/1.1 {} {}\r\n{}\r\n\r\n{}\r\n",
            status_code, status_message, headers, contents
        );

        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        return;
    }

    // println!("{}", request_raw);
    // println!("Request has {} bytes", request_raw.len());

    if !request_raw.starts_with("GET") {
        status_code = 501;
        status_message = String::from("Not Implemented");
        contents = Box::new(String::from("501 - Not Implemented"));
        headers = Box::new(format!(
            "Content-Length: {}\r\nConnection: Keep-Alive\r\nDate: {}\r\nServer: rust\r\nContent-Type: text/html; charset=utf-8",
            contents.len() + 2,
            Utc::now().format("%a, %b %e %Y %T GMT").to_string()
        ));
        let response = format!(
            "HTTP/1.1 {} {}\r\n{}\r\n\r\n{}\r\n",
            status_code, status_message, headers, contents
        );

        stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        return;
    }

    if let Some(x) = files.get("/index.html") {
        contents = Box::new(x.clone());
        status_code = 200;
        status_message = String::from("OK");
    }

    headers = Box::new(format!(
        "Content-Length: {}\r\nConnection: Keep-Alive\r\nDate: {}\r\nServer: rust\r\nContent-Type: text/html; charset=utf-8",
        contents.len() + 2,
        Utc::now().format("%a, %b %e %Y %T GMT").to_string()
    ));
    let response = format!(
        "HTTP/1.1 {} {}\r\n{}\r\n\r\n{}\r\n",
        status_code, status_message, headers, contents
    );

    stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

fn main() {
    let listener_v4 = TcpListener::bind("127.0.0.1:7878").unwrap();
    // let pool = ThreadPool::new(num_cpus::get() << num_cpus::get());
    let pool = ThreadPool::new(num_cpus::get());
    let mut files = HashMap::new();

    if Path::new("index.html").exists() {
        let mut contents = String::new();
        File::open("index.html")
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        files.insert(String::from("/index.html"), contents);
    }

    for stream in listener_v4.incoming() {
        let stream = stream.unwrap();
        let files = files.clone();
        pool.execute(|| {
            handle_connection(stream, files);
        });
    }

    process::exit(0);
}
