extern crate env_logger;
extern crate log;
extern crate chrono;
extern crate num_cpus;

use chrono::prelude::*;
use signal_hook::consts::{SIGINT, SIGQUIT};
use signal_hook::iterator::Signals;
use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::SendError;
use std::thread;
use std::time::Duration;
use log::{debug, error, info};
use env_logger::Env;

const CRLF: &str = "\r\n";

pub enum WorkerMessage {
    NewJob(Job),
    Terminate,
}

pub enum ConnectionHandlerMessage {
    NewConnection(TcpStream),
    Terminate,
}

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<WorkerMessage>,
}

pub trait FnBox {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<F>) {
        (*self)();
    }
}

pub type Job = Box<dyn FnBox + Send + 'static>;

impl ThreadPool {
    /// Create a new `ThreadPool`.
    ///
    /// The size is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if the size is zero.
    #[must_use] pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        ThreadPool { workers, sender }
    }

    /// Execute a function on the `ThreadPool`.
    ///
    /// # Errors
    /// A return value of `Err` means that the data will never be received, but a return value of
    /// `Ok` does not mean that the data will be received. It is possible for the corresponding
    /// receiver to hang up immediately after this function returns `Ok`.
    pub fn execute<F>(&self, f: F)  -> Result<(), SendError<WorkerMessage>>
        where F: FnOnce() + Send + 'static, {

        self.sender.send(WorkerMessage::NewJob(Box::new(f)))
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        info!("Sending terminate message to all workers.");

        for _ in &mut self.workers {
            self.sender.send(WorkerMessage::Terminate).unwrap();
        }

        info!("Shutting down all workers.");

        for worker in &mut self.workers {
            debug!("Shutting down worker thread {}", worker.id);

            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(
        id: usize,
        receiver: Arc<Mutex<mpsc::Receiver<WorkerMessage>>>
    ) -> Worker {
        let thread = thread::spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                WorkerMessage::NewJob(job) => {
                    job.call_box();
                }
                WorkerMessage::Terminate => {
                    debug!("Worker {id} was told to terminate.");
                    break;
                }
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}

pub struct ConnectionHandler {
    thread: thread::JoinHandle<()>,
}

impl ConnectionHandler {
    /// Creates new `ConnectionHandler` instance.
    ///
    /// # Panics
    /// When `thread_count` is less then 1
    #[must_use] pub fn new(
        thread_count: usize,
        receiver: Arc<Mutex<mpsc::Receiver<ConnectionHandlerMessage>>>,
    ) -> ConnectionHandler {
        assert!(thread_count > 0);

        info!("starting with {thread_count} threads");
        let thread_pool = ThreadPool::new(thread_count);
        let thread = thread::spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                ConnectionHandlerMessage::NewConnection(stream) => {
                    thread_pool.execute(|| {
                        handle_connection(stream);
                    }).unwrap();
                }
                ConnectionHandlerMessage::Terminate => {
                    debug!("Connection handler was told to terminate.");
                    break;
                }
            }
        });

        ConnectionHandler {
            thread,
         }
    }
}

#[must_use] fn new_date_time_http_rfc() -> String {
    Utc::now().format("%a, %b %e %Y %T GMT").to_string()
}

#[must_use] fn compose_http_response_headers(content_len: usize, content_type: &str) -> String {
    format!(
        "Content-Length: {content_len}{CRLF}Connection: Keep-Alive{CRLF}Date: {date}{CRLF}Content-Type: {content_type}",
        date = new_date_time_http_rfc()
    )
}

#[must_use] fn compose_http_response(status_code: u16, status_message: &str, headers: &str, body: &str) -> String {
    format!("HTTP/1.1 {status_code} {status_message}{CRLF}{headers}{CRLF}{CRLF}{body}")
}

fn handle_connection(mut stream: TcpStream) {
    let mut bytes_read: usize = 0;
    let mut request_raw = String::new();

    let mut status_code: u16 = 404;
    let mut status_message = "Not Found";
    let headers: String;
    let mut body = "404 - Not Found";

    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(10))).unwrap();

    // TODO: read up to upper bound and reject connection when reached
    {
        let mut buffer = [0; 1024];
        while bytes_read < 50_000_000 && stream.read(&mut buffer).unwrap() <= 1024 {
            let buffer_raw = String::from_utf8_lossy(&buffer[..]);
            let is_last_buffer = buffer_raw.ends_with('\0');
            let buffer_raw = buffer_raw.trim_end_matches('\0');
            bytes_read += buffer_raw.len();
            request_raw.push_str(buffer_raw);
            if is_last_buffer {
                break;
            }
        }
    }

    if bytes_read >= 50_000_000 {
        status_code = 413;
        status_message = "Payload Too Large";
        body = "413 - Payload Too Large\n";
        headers = compose_http_response_headers(body.len(), "text/html; charset=utf-8");
        let response = compose_http_response(status_code, status_message, headers.as_str(), body);

        let _ = stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        return;
    }

    // debug!("{}", request_raw);
    debug!("Request has {} bytes", request_raw.len());

    if !request_raw.starts_with("GET") {
        status_code = 501;
        status_message = "Not Implemented";
        body = "501 - Not Implemented\n";
        headers = compose_http_response_headers(body.len(), "text/html; charset=utf-8");
        let response = compose_http_response(status_code, status_message, headers.as_str(), body);

        let _ = stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        return;
    }

    let mut files = HashMap::new();
    if Path::new("index.html").exists() {
        let mut contents = String::new();
        File::open("index.html")
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        files.insert(String::from("/index.html"), contents.clone());
    }
    // files.insert(String::from("/index.html"), String::from("Hello world!"));

    if let Some(x) = files.get("/index.html") {
        body = x.as_str();
        status_code = 200;
        status_message = "OK";
    }

    headers = compose_http_response_headers(body.len(), "text/html; charset=utf-8");
    let response = compose_http_response(status_code, status_message, headers.as_str(), body);

    let _ = stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .format_target(false)
        .init();

    let (sender, receiver) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(receiver));
    let sender_signal = sender.clone();
    let mut signals = Signals::new([SIGINT, SIGQUIT]).unwrap();
    thread::spawn(move || {
        for sig in signals.forever() {
            info!("Received process signal {sig:?}");
            sender_signal.send(ConnectionHandlerMessage::Terminate).unwrap_or_default();

            // Give the application 10 seconds to gracefully shutdown
            thread::sleep(Duration::from_secs(10));
            error!("application did not shutdown within 10 seconds, force terminate");
            process::exit(1);
        }
    });

    let thread_count = num_cpus::get() * 2;
    //let thread_count = num_cpus::get() << num_cpus::get();

    let bind_addr = "127.0.0.1:8000";
    info!("listening on {bind_addr}");
    let listener = TcpListener::bind(bind_addr).unwrap();

    let connection_handler = ConnectionHandler::new(
        thread_count,
        receiver,
    );
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    sender.send(ConnectionHandlerMessage::NewConnection(stream)).unwrap();
                },
                Err(err) => {
                    error!("incoming connection error: {err}");
                }
            }
        }
    });

    connection_handler.thread.join().unwrap();
}

