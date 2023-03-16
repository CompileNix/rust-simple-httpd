extern crate env_logger;
extern crate log;
extern crate num_cpus;

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
use log::{trace, debug, info, warn, error};
use env_logger::Env;
use std::io::ErrorKind;
use indoc::formatdoc;

const BUFFER_SIZE: usize = 32;

enum WorkerMessage {
    NewJob(Job),
    Terminate,
}

enum ConnectionHandlerMessage {
    NewConnection(TcpStream),
    Terminate,
}

struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<WorkerMessage>,
}

trait FnBox {
    fn call_box(self: Box<Self>, worker_id: usize);
}

impl<F: FnOnce(usize)> FnBox for F {
    fn call_box(self: Box<F>, worker_id: usize) { (*self)(worker_id); }
}

type Job = Box<dyn FnBox + Send + 'static>;

impl ThreadPool {
    /// Create a new `ThreadPool`.
    ///
    /// The size is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if the size is zero.
    #[must_use] fn new(size: usize) -> ThreadPool {
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
    fn execute<F>(&self, f: F)  -> Result<(), SendError<WorkerMessage>>
        where F: FnOnce(usize) + Send + 'static, {

        self.sender.send(WorkerMessage::NewJob(Box::new(f)))
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        trace!("Sending terminate message to all workers.");

        for _ in &mut self.workers {
            self.sender.send(WorkerMessage::Terminate).unwrap();
        }

        debug!("Waiting for all workers to shut down.");

        for worker in &mut self.workers {
            trace!("Shutting down worker thread {}", worker.id);

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
    #[must_use] fn new(
        id: usize,
        receiver: Arc<Mutex<mpsc::Receiver<WorkerMessage>>>
    ) -> Worker {
        let thread = thread::Builder::new()
            .name(format!("Worker {id}"))
            .spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                WorkerMessage::NewJob(job) => {
                    job.call_box(id);
                }
                WorkerMessage::Terminate => {
                    trace!("Worker {id} was told to terminate.");
                    break;
                }
            }
        }).unwrap();

        Worker {
            id,
            thread: Some(thread),
        }
    }
}

struct ConnectionHandler {
    thread: thread::JoinHandle<()>,
}

impl ConnectionHandler {
    /// Creates new `ConnectionHandler` instance.
    ///
    /// # Panics
    /// When `thread_count` is less then 1
    #[must_use] fn new(
        thread_count: usize,
        receiver: Arc<Mutex<mpsc::Receiver<ConnectionHandlerMessage>>>,
    ) -> ConnectionHandler {
        assert!(thread_count > 0);

        info!("starting with {thread_count} threads");
        let thread_pool = ThreadPool::new(thread_count);
        let thread = thread::Builder::new()
            .name("ConnectionHandler".into())
            .spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                ConnectionHandlerMessage::NewConnection(stream) => {
                    thread_pool.execute(|worker_id| {
                        trace!("Worker {worker_id} received a new job from the thread pool of the connection handler");
                        handle_connection(stream, worker_id);
                    }).unwrap();
                }
                ConnectionHandlerMessage::Terminate => {
                    trace!("Connection handler was told to terminate.");
                    break;
                }
            }
        }).unwrap();

        ConnectionHandler {
            thread,
         }
    }
}

/// This function searches for the end of HTTP headers in the input string `content`.
/// It returns an `Option<usize>` type that represents the index of the end of HTTP headers,
/// if found, or `None` if the end of HTTP headers is not found.
///
/// # Arguments
///
/// * `content` - A string slice which represents the content to search for the end of HTTP headers.
///
/// # Returns
///
/// * An optional index value which represents the index of the end of HTTP headers in the input `content` string.
/// If the end of HTTP headers is found, the function returns `Some(index)` where `index` is the index of the end of HTTP headers.
/// If the end of HTTP headers is not found, the function returns `None`.
///
/// # Examples
///
/// ```
/// let content = "HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nHello world\n";
/// let end_of_headers = contains_end_of_http_headers(content);
///
/// assert_eq!(end_of_headers, Some(35));
/// ```
fn contains_end_of_http_headers(content: &str) -> Option<usize> {
    content.rfind("\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_contains_end_of_http_headers_found() {
        let content = indoc!{"
            HTTP/1.1 200 OK\r
            Content-Length: 10\r
            \r
            Hello world\
        "};
        let end_of_headers = contains_end_of_http_headers(content);
        assert_eq!(end_of_headers, Some(35));
    }

    #[test]
    fn test_contains_end_of_http_headers_not_found() {
        let content = indoc!{"
            HTTP/1.1 200 OK\r
            Content-Length: 10"};
        let end_of_headers = contains_end_of_http_headers(content);
        assert_eq!(end_of_headers, None);
    }

    #[test]
    fn test_contains_end_of_http_headers_empty_string() {
        let content = "";
        let end_of_headers = contains_end_of_http_headers(content);
        assert_eq!(end_of_headers, None);
    }

    #[test]
    fn test_partial_contains_end_of_http_headers_found() {
        let content = indoc!{"
            Content-Length: 10\r
            \r
            Hello world\
        "};
        let end_of_headers = contains_end_of_http_headers(content);
        assert_eq!(end_of_headers, Some(18));
    }

    #[test]
    fn test_contains_end_of_http_headers_long_string() {
        let content = indoc!{"
            HTTP/1.1 200 OK\r
            Content-Type: text/plain\r
            This is an example response body that contains some text.\r
            It includes multiple lines of text separated by newline characters.\r
            The end of HTTP headers is indicated by two consecutive newline characters.\r
            \r
            Lorem ipsum dolor sit amet.
        "};
        let end_of_headers = contains_end_of_http_headers(content);
        assert_eq!(end_of_headers, Some(246));
    }
}

#[must_use] fn new_date_time_http_rfc() -> String {
    time::OffsetDateTime::now_utc().to_string()
}

#[must_use] fn compose_http_response_headers(content_len: usize, content_type: &str) -> String {
    let headers = formatdoc!{"
        Content-Length: {content_len}\r
        Connection: Keep-Alive\r
        Date: {date}\r
        Content-Type: {content_type}",
        date = new_date_time_http_rfc(),
    };
    headers
}

#[must_use] fn compose_http_response(status_code: u16, status_message: &str, headers: &str, body: &str) -> String {
    let http_response = formatdoc!{"
        HTTP/1.1 {status_code} {status_message}\r
        {headers}\r
        \r
        {body}"};
    http_response
}

fn handle_connection(mut stream: TcpStream, worker_id: usize) {
    // Set stream read and write timeouts
    if let Err(err) = stream.set_read_timeout(Some(Duration::from_millis(3_000))) {
        warn!("Worker {worker_id}: set_read_timeout call failed: {err}");
        return;
    }
    if let Err(err) = stream.set_write_timeout(Some(Duration::from_millis(1_000))) {
        warn!("Worker {worker_id}: set_write_timeout call failed: {err}");
        return;
    }

    // Read http headers from stream
    let request_header_limit_bytes = 4096;
    let mut request_headers_bytes_read: u64 = 0;
    let mut request_headers = String::new();
    let mut request_body = String::new();
    let mut bytes_body_read = 0;
    loop {
        // init read buffer
        let mut buffer = [0; BUFFER_SIZE];

        // try to read data from remote into buffer
        let bytes_read = match stream.read(&mut buffer) {
            Ok(bytes_read) => {
                // successful read
                bytes_read
            }
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock) => {
                // socket is not ready to be read from at the moment
                // ignore `WouldBlock` error and continue loop
                continue;
            }
            Err(err) => {
                warn!("Worker {worker_id}: 🤷 Error while reading from remote, aborting connection. Here is the error we got: {err}");
                return;
            }
        };

        request_headers_bytes_read += bytes_read as u64;
        trace!("Worker {worker_id}: buffer bytes_read: {bytes_read}");

        // check if we reached our limit for reading client http headers and bail out of the
        // connection if that's the case.
        if request_headers_bytes_read > request_header_limit_bytes {
            warn!("Worker {worker_id}: request header size limit of {header_limit_bytes} has been exceeded by {exceeded_by_amount}, we've read a total of {bytes_read}. Aborting connection ...",
                      header_limit_bytes = request_header_limit_bytes,
                      exceeded_by_amount = request_headers_bytes_read - request_header_limit_bytes,
                      bytes_read = bytes_read
            );
            return;
        }

        // make sure we received only valid utf8 data from remote
        let mut header_buffer_data_read = match String::from_utf8(buffer.to_vec()) {
            Ok(value) => {
                // read data from remote is valid utf8
                value
            }
            Err(err) => {
                // TODO: use HTTP 400
                debug!("Worker {worker_id}: HTTP 400: remote sent some invalid utf8: {err}");
                return;
            }
        };
        // Remove null-bytes
        header_buffer_data_read = String::from(header_buffer_data_read.trim_matches('\0'));

        trace!("Worker {worker_id}: buffer content:\n{header_buffer_data_read}");

        // add read buffer data to `headers` String
        request_headers.push_str(&header_buffer_data_read);

        // check if recently read data (2x size of buffer at the end) contains end-of-headers
        // indicator (2x "\r\n") and stop reading from remote stream (for new headers).
        match contains_end_of_http_headers(&request_headers) {
            None => {}
            Some(end_of_headers_byte_index) => {
                trace!("Worker {worker_id}: remote seems to be complete with sending their http request headers");

                // assemble request body from leftovers of while reading headers
                // the ` + 4` is done to skip over \r\n\r\n at the end of headers
                let first_parts_of_request_body_string = request_headers.clone();
                let first_parts_of_request_body = first_parts_of_request_body_string.as_str().get(end_of_headers_byte_index + 4..).unwrap_or_default();
                request_headers = request_headers.as_str().get(..end_of_headers_byte_index).unwrap().to_string();
                trace!("Worker {worker_id}: first_parts_of_request_body = {first_parts_of_request_body}");
                trace!("Worker {worker_id}: first_parts_of_request_body.len() = {first_parts_of_request_body_len}", first_parts_of_request_body_len = first_parts_of_request_body.len());
                #[allow(clippy::needless_late_init)] // false positive
                let mut rest_of_data_from_remote_buffer = String::new();
                // try to read data from remote if last buffer was full
                // TODO: parse request headers before this and extract request body size
                if bytes_read == BUFFER_SIZE {
                    // TODO: we're currently simply reading the rest of the stream
                    {
                        let mut bytes_read = 0;
                        let mut buffer = [0; BUFFER_SIZE];
                        while bytes_read < 50_000_000 && stream.read(&mut buffer).unwrap() <= BUFFER_SIZE {
                            // TODO: nah... works for now but is messy and prob error prone anyway 😆
                            let buffer_raw = String::from_utf8_lossy(&buffer[..]);
                            let is_last_buffer = buffer_raw.ends_with('\0');
                            let buffer_raw = buffer_raw.trim_end_matches('\0');
                            let buffer_raw_len = buffer_raw.len();
                            bytes_read += buffer_raw_len;
                            trace!("Worker {worker_id}: We got some request body data, with length {buffer_raw_len} bytes:\n{buffer_raw}");
                            rest_of_data_from_remote_buffer.push_str(buffer_raw);
                            if is_last_buffer {
                                break;
                            }
                            buffer = [0; BUFFER_SIZE];
                        }
                        bytes_body_read += bytes_read;
                    }
                } else {
                    bytes_body_read = 0;
                }
                if bytes_body_read > 0 {
                    trace!("Worker {worker_id}: rest_of_request_body = {rest_of_request_body}", rest_of_request_body = String::from_utf8(rest_of_data_from_remote_buffer.clone().into()).unwrap());
                    trace!("Worker {worker_id}: rest_of_request_body_len = {rest_of_request_body_len}", rest_of_request_body_len = String::from_utf8(rest_of_data_from_remote_buffer.clone().into()).unwrap().len());
                    request_body.push_str(first_parts_of_request_body);
                    request_body.push_str(String::from_utf8(rest_of_data_from_remote_buffer.into()).unwrap().as_str());
                } else {
                    request_body.push_str(first_parts_of_request_body);
                }

                break;
            }
        }
    }

    trace!("Worker {worker_id}: Print request headers:\n{request_headers}");
    let mut file = File::create("request_headers.txt").unwrap();
    file.write_all(request_headers.as_bytes()).unwrap();
    trace!("Worker {worker_id}: Print request body:\n{request_body}");
    let mut file = File::create("request_body.txt").unwrap();
    file.write_all(request_body.as_bytes()).unwrap();

    if bytes_body_read >= 50_000_000 {
        let status_code = 413;
        let status_message = "Payload Too Large";
        let body = "413 - Payload Too Large\n";
        let response_headers = compose_http_response_headers(body.len(), "text/plain; charset=utf-8");
        let response = compose_http_response(status_code, status_message, response_headers.as_str(), body);

        let _ = stream.write(response.as_bytes()).unwrap();
        stream.flush().unwrap();
        return;
    }

    debug!("Worker {worker_id}: {request}", request = request_headers.lines().take(1).collect::<String>());

    if !request_headers.starts_with("GET") {
        let status_code = 501;
        let status_message = "Method Not Implemented";
        let body = "501 - Method Not Implemented\n";
        let response_headers = compose_http_response_headers(body.len(), "text/plain; charset=utf-8");
        let response = compose_http_response(status_code, status_message, response_headers.as_str(), body);

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
        files.insert(String::from("/index.html"), contents);
    }

    let mut status_code = 404;
    let mut status_message = "Not Found";
    let mut body = String::from("Not Found");
    if let Some(x) = files.get("/index.html") {
        body = x.into();
        status_code = 200;
        status_message = "OK";
    }

    let response_headers = compose_http_response_headers(body.len(), "text/html; charset=utf-8");
    let response = compose_http_response(status_code, status_message, response_headers.as_str(), body.as_str());

    let _ = stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .format_target(false)
        .format_indent(Some("[0000-00-00T00:00:00.000Z INFO ] ".len()))
        .init();

    let (sender, receiver) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(receiver));
    let sender_signal = sender.clone();
    let _ = thread::Builder::new()
        .name("ProcessSignalHandler".into())
        .spawn(move || {
        let mut signals = Signals::new([SIGINT, SIGQUIT]).unwrap();
        for sig in signals.forever() {
            info!("Received process signal {sig:?}");
            sender_signal.send(ConnectionHandlerMessage::Terminate).unwrap_or_default();

            // Give the application 10 seconds to gracefully shutdown
            thread::sleep(Duration::from_secs(10));
            error!("application did not shutdown within 10 seconds, force terminate");
            process::exit(1);
        }
    });

    let worker_count = num_cpus::get();

    let bind_addr = "127.0.0.1:8000";
    info!("listening on {bind_addr}");
    let listener = TcpListener::bind(bind_addr).unwrap();

    let connection_handler = ConnectionHandler::new(
        worker_count,
        receiver,
    );
    let _ = thread::Builder::new()
        .name("TcpListener".into())
        .spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    sender.send(ConnectionHandlerMessage::NewConnection(stream)).unwrap();
                },
                Err(err) => {
                    warn!("incoming connection error: {err}");
                }
            }
        }
    });

    connection_handler.thread.join().unwrap();
}

