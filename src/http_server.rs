use crate::config::Config;
#[cfg(feature = "log-trace")]
use crate::util::highlighted_hex_vec;
use crate::{util, Level};
use crate::{trace, debug, verb, warn, info};
use anyhow::{anyhow, Result};
use indoc::formatdoc;
use std::any::Any;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::SendError;
use std::thread;
use std::sync::{mpsc, Arc, Mutex};

enum WorkerMessage {
    NewJob(Job),
    Shutdown,
}

pub(crate) enum ConnectionHandlerMessage {
    NewConnection(TcpStream),
    Shutdown,
}

struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<WorkerMessage>,
    config: Config,
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
    #[must_use] fn new(config: Config) -> ThreadPool {
        let cfg = &config;

        let worker_count = util::available_parallelism_capped_at(config.workers);
        assert!(worker_count > 0);
        info!(cfg, "starting with {worker_count} threads");

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(worker_count);

        for id in 0..worker_count {
            workers.push(Worker::new(id, Arc::clone(&receiver), cfg));
        }

        ThreadPool { workers, sender, config }
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
        let cfg = &self.config;
        debug!(cfg, "Thread pool: received shutdown signal");

        trace!(cfg, "Thread pool: sending shutdown message to all {} workers", self.workers.len());
        for _ in &mut self.workers {
            self.sender.send(WorkerMessage::Shutdown).unwrap();
        }

        debug!(cfg, "Thread pool: waiting for all workers to stop");
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
        debug!(cfg, "Thread pool: all workers completed");
    }
}

struct Worker {
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    #[must_use] fn new(
        id: usize,
        receiver: Arc<Mutex<mpsc::Receiver<WorkerMessage>>>,
        config: &Config,
    ) -> Worker {
        let thread_cfg = config.clone();
        let thread = thread::Builder::new()
            .name(format!("Worker {id}"))
            .spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                WorkerMessage::NewJob(job) => {
                    job.call_box(id);
                }
                WorkerMessage::Shutdown => {
                    trace!(&thread_cfg, "Worker {id}: shutdown");
                    break;
                }
            }
        }).unwrap();

        Worker {
            thread: Some(thread),
        }
    }
}

#[derive(Debug)]
struct ConnectionHandler {
    thread: thread::JoinHandle<()>,
}

impl ConnectionHandler {
    /// Creates new `ConnectionHandler` instance.
    ///
    /// # Panics
    /// When `thread_count` is less then 1
    #[must_use]
    fn new(
        receiver: Arc<Mutex<mpsc::Receiver<ConnectionHandlerMessage>>>,
        config: &Config,
    ) -> ConnectionHandler {
        let thread_pool = ThreadPool::new(config.clone());

        let connection_handler_config = config.clone();
        let thread = thread::Builder::new()
            .name("ConnectionHandler".into())
            .spawn(move || loop {
            let cfg = connection_handler_config.clone();
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                ConnectionHandlerMessage::NewConnection(mut stream) => {
                    verb!(&cfg, "ConnectionHandler: received a new connection from HTTP Server");
                    thread_pool.execute(move |worker_id| {
                        trace!(&cfg, "[{worker_id}]: received a new job from the thread pool of the connection handler");
                        HttpServer::handle_connection(&mut stream, worker_id, &cfg);
                    }).unwrap();
                }
                ConnectionHandlerMessage::Shutdown => {
                    verb!(&cfg, "ConnectionHandler: received shutdown signal");
                    break;
                }
            }
        }).unwrap();

        ConnectionHandler { thread }
    }
}

#[derive(Debug)]
pub struct HttpServer {
    connection_handler: ConnectionHandler,
}

impl HttpServer {
    pub fn new(
        config: &Config,
        receiver: Arc<Mutex<mpsc::Receiver<ConnectionHandlerMessage>>>,
        sender: mpsc::Sender<ConnectionHandlerMessage>,
    ) -> HttpServer {
        let config = config.clone();
        let cfg = &config;

        let listener = TcpListener::bind(config.bind_addr.clone()).expect(&format!("Can't bind to {}", cfg.bind_addr));
        info!(cfg, "listening on {}", cfg.bind_addr);

        let connection_handler = ConnectionHandler::new(
            receiver,
            &config.clone(),
        );

        let listener_config = config.clone();
        let listener_handler = listener.try_clone().unwrap();
        let _ = thread::Builder::new()
            .name("TcpListener".into())
            .spawn(move || {
            let cfg = &listener_config;
            for stream in listener_handler.incoming() {
                match stream {
                    Ok(stream) => {
                        sender.send(ConnectionHandlerMessage::NewConnection(stream)).unwrap();
                    },
                    Err(err) => {
                        warn!(cfg, "incoming connection error: {err}");
                    }
                }
            }
        });

        HttpServer { connection_handler }
    }

    pub fn serve(self) -> Result<(), Box<dyn Any + Send>> {
        self.connection_handler.thread.join()
    }

    /// Searches for the first occurrence of a specific byte sequence in a given slice of bytes.
    ///
    /// This function is specifically designed to find the end-of-header (EOH) marker in HTTP
    /// headers, which is represented by the byte sequence "\r\n\r\n". It iterates over the slice
    /// and checks for the presence of this sequence.
    ///
    /// # Arguments
    ///
    /// * `content` - A slice of bytes (`&[u8]`) in which to search for the EOH marker.
    ///
    /// # Returns
    ///
    /// Returns an `Option<usize>`:
    /// - `Some(usize)` - If the EOH marker is found, returns the index at which the marker begins, else returns a `None`.
    /// - `None` - If the EOH marker is not found or the given byte slice is smaller than the EOH marker.
    ///
    /// # Examples
    ///
    /// ```
    /// let data = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nBody";
    /// let index = bytes_contain_eoh(data);
    /// assert_eq!(index, Some(41));
    /// ```
    pub(crate) fn bytes_contain_eoh(content: &[u8]) -> Option<usize> {
        let eoh_sequence = b"\r\n\r\n"; // 0x0d, 0x0a, 0x0d, 0x0a

        if content.len() < eoh_sequence.len() {
            return None;
        }

        // Search for the EOH sequence in the content
        let position = content
            .windows(eoh_sequence.len())
            .position(|window| window == eoh_sequence);

        // Return the position found, or None if not found
        position
    }

    #[must_use]
    fn new_date_time_http_rfc() -> String {
        let now = time::OffsetDateTime::now_utc();
        let format = time::format_description::parse_borrowed::<2>(
            "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT",
        )
        .expect("Error creating HTTP timestamp");
        now.format(&format).unwrap_or_default()
    }

    #[must_use]
    fn compose_http_response_headers(content_len: usize, content_type: &str) -> String {
        let headers = formatdoc! {"
        server: rust-simple-httpd\r
        date: {date}\r
        content-type: {content_type}\r
        content-length: {content_len}\r
        connection: close",
            date = HttpServer::new_date_time_http_rfc(),
        };
        headers
    }

    #[must_use]
    fn compose_http_response(
        status_code: u16,
        status_message: &str,
        headers: &str,
        body: &str,
    ) -> String {
        let http_response = formatdoc! {"
        HTTP/1.1 {status_code} {status_message}\r
        {headers}\r
        \r
        {body}"};
        http_response
    }

    fn read_stream_into(
        buffer: &mut [u8],
        stream: &mut TcpStream,
        worker_id: usize,
        config: &Config,
    ) -> anyhow::Result<usize> {
        let cfg = config;

        // Set stream read and write timeouts
        if let Err(err) = stream.set_read_timeout(Some(cfg.buffer_read_client_timeout)) {
            warn!(cfg, "[{worker_id}]: set_read_timeout call failed: {err}");
            return Err(anyhow!(err));
        }
        if let Err(err) = stream.set_write_timeout(Some(cfg.buffer_write_client_timeout)) {
            warn!(cfg, "[{worker_id}]: set_write_timeout call failed: {err}");
            return Err(anyhow!(err));
        }

        // let result_timeout = tokio::time::timeout(self.config.buffer_read_client_timeout, stream.read(buffer)).await;

        // let result_read = match result_timeout {
        //     Ok(result_read) => result_read,
        //     Err(err) => {
        //         warn!(cfg, "Read timed out: {err}");
        //         return Err(anyhow!(err));
        //     }
        // };

        // let bytes_read = match result_read {
        //     Ok(bytes_read) => bytes_read,
        //     Err(err) => {
        //         warn!(cfg, "[{worker_id}]: 🤷 Error while reading from remote, aborting connection. Here is the error we got: {err}");
        //         return Err(anyhow!(err));
        //     }
        // };

        // try to read data from remote into buffer
        let bytes_read = match stream.read(buffer) {
            Ok(bytes_read) => {
                // successful read
                bytes_read
            }
            Err(err) if matches!(err.kind(), std::io::ErrorKind::WouldBlock) => {
                // socket is not ready to be read from at the moment
                // ignore `WouldBlock` error and continue loop
                return Ok(0);
            }
            Err(err) => {
                warn!(cfg, "[{worker_id}]: 🤷 Error while reading from remote, aborting connection. Here is the error we got: {err}");
                return Err(anyhow!(err));
            }
        };

        debug!(
            cfg,
            "[{worker_id}]: stream.read(buffer({len})): returned {bytes_read} bytes",
            len = buffer.len()
        );

        Ok(bytes_read)
    }

    fn send_basic_status_response(
        stream: &mut TcpStream,
        status_code: u16,
        status_message: &str,
    ) {
        let body = format!("{status_message}\n");
        let response_headers =
            Self::compose_http_response_headers(body.len(), "text/plain; charset=utf-8");
        let response = Self::compose_http_response(
            status_code,
            status_message,
            response_headers.as_str(),
            &body,
        );

        let _ = stream.write(response.as_bytes());
        stream.flush().ok();
    }

    // TODO: continue work
    #[allow(
        unused_variables,
        unused_assignments,
        clippy::unwrap_used,
        clippy::too_many_lines
    )]
    pub fn handle_connection(stream: &mut TcpStream, worker_id: usize, config: &Config) {
        let cfg = config;

        let mut request_headers = vec![0; config.buffer_client_receive_size];
        let request_body: Vec<u8>;
        let bytes_body_read: usize = 0;
        let mut request_data = vec![];
        let request_body_byte_index_start;
        let mut buffer = vec![0; config.buffer_client_receive_size];
        let mut request_header_limit_bytes_exceeded = false;
        let mut request_header_incomplete = false;

        // Reading request headers
        debug!(cfg, "[{worker_id}]: begin reading stream into buffer until EOH");
        trace!(
            cfg,
            r#"[{worker_id}]: EOH sequence is "\r\n\r\n" (0x0d, 0x0a, 0x0d, 0x0a)"#
        );
        loop {
            // Read into buffer
            let Ok(bytes_read) = Self::read_stream_into(&mut buffer, stream, worker_id, cfg) else {
                warn!(
                    cfg,
                    "[{worker_id}]: reading the stream failed, so we are going to bail out of this connection"
                );
                return;
            };

            trace!(
                cfg,
                "[{worker_id}]: buffer data: {}",
                highlighted_hex_vec(&buffer, request_data.len(), cfg)
            );

            // Trim null bytes, in case the buffer wasn't filled
            let Some(buffer_trimmed) = buffer.get(..bytes_read) else {
                warn!(cfg, "[{worker_id}]: We could not trim the input buffer. The buffer has a length of {buffer_size} and we got {bytes_read} bytes.", buffer_size = config.buffer_client_receive_size);
                return;
            };

            // Check if REQUEST_HEADER_LIMIT_BYTES has been exceeded
            // FIXME: headers might end within the current buffer, this is not checked
            if request_data.len() + buffer_trimmed.len() >= config.request_header_limit_bytes {
                request_header_limit_bytes_exceeded = true;
                break;
            }

            // append buffer to request_data and check if the client is done with sending request headers
            request_data.extend(buffer_trimmed);
            if let Some(eoh_byte_index) = Self::bytes_contain_eoh(request_data.as_slice()) {
                debug!(cfg, "[{worker_id}]: EOH sequence found at {eoh_byte_index}");
                request_headers = request_data.get(..eoh_byte_index).unwrap().to_vec();
                request_body_byte_index_start = eoh_byte_index + 4; // +4 to skip over the \r\n\r\n at the end
                break;
            }

            // since we haven't found the end-of-headers marker (\\r\\n\\r\\n), the request can't be valid / complete
            if bytes_read == 0 || bytes_read < config.buffer_client_receive_size {
                request_header_incomplete = true;
                break;
            }
        }
        buffer.fill(0);

        if request_header_incomplete {
            warn!(
                cfg,
                r#"[{worker_id}]: Since we haven't found the "\r\n\r\n" marker but the remote indicated that they are done sending data we can conclude that the request must be incomplete because all http requests must contain the "\r\n\r\n" sequence at least once."#
            );
            Self::send_basic_status_response(stream, 400, "Bad Request");
            return;
        }

        if request_header_limit_bytes_exceeded {
            warn!(cfg, "[{worker_id}]: Request header size limit of {request_header_limit_bytes} has been exceeded. Aborting connection...", request_header_limit_bytes = config.request_header_limit_bytes);
            Self::send_basic_status_response(stream, 400, "Bad Request");
            return;
        }

        //print!("{:02X?}\n", request_data);
        //io::stdout().flush().unwrap();

        // TODO: read request body and determined if the body should also be fetched.

        let request_headers = String::from_utf8_lossy(&request_headers);
        let request_body = String::new();

        verb!(cfg, "[{worker_id}]: Request headers:\n{request_headers}");
        // let mut file = File::create("request_headers.txt").unwrap();
        // file.write_all(request_headers.as_bytes()).unwrap();
        verb!(cfg, "[{worker_id}]: Request body:\n{request_body}");
        // let mut file = File::create("request_body.txt").unwrap();
        // file.write_all(request_body.as_bytes()).unwrap();

        if bytes_body_read >= 50_000_000 {
            Self::send_basic_status_response(stream, 413, "Payload To Large");
            return;
        }

        // log.debug(&format!(
        //     "Worker: {request}",
        //     request = request_headers.lines().take(1).collect::<String>()
        // ));

        //if !request_headers.starts_with("GET") {
        //    let status_code = 501;
        //    let status_message = "Method Not Implemented";
        //    let body = "501 - Method Not Implemented\n";
        //    let response_headers = compose_http_response_headers(
        //        body.len(),
        //        "text/plain;
        //charset=utf-8",
        //    );
        //    let response =
        //        compose_http_response(status_code, status_message, response_headers.as_str(), body);

        //    let _ = stream.write(response.as_bytes()).await.unwrap();
        //    stream.flush().await.unwrap();
        //    return Ok(());
        //}

        // let mut files = HashMap::new();
        // if Path::new("index.html").exists() {
        //     let mut contents = String::new();
        //     File::open("index.html")
        //         .expect(r#"Unable to open file "index.html" for reading"#)
        //         .read_to_string(&mut contents)
        //         .expect(r#"error while reading "index.html" into a `String` type"#);
        //     files.insert(String::from("/index.html"), contents);
        // }

        // let mut status_code = 404;
        // let mut status_message = "Not Found";
        // let mut body = String::from("Not Found\n");
        // let mut content_type = "text/plain; charset=utf-8";
        // if let Some(x) = files.get("/index.html") {
        //     body = x.into();
        //     status_code = 200;
        //     status_message = "OK";
        //     content_type = "text/html; charset=utf-8";
        // }
        let status_code = 200;
        let status_message = "OK";
        let body = String::from("<!DOCTYPE html>\n<html lang=\"en\">\n\n<head>\n    <meta charset=\"utf-8\">\n    <title>Hello!</title>\n</head>\n\n<body>\n    <h1>Hello!</h1>\n    <p>Hi from Rust</p>\n</body>\n\n</html>\n");
        let content_type = "text/plain; charset=utf-8";

        let response_headers = Self::compose_http_response_headers(body.len(), content_type);
        let response = Self::compose_http_response(
            status_code,
            status_message,
            response_headers.as_str(),
            body.as_str(),
        );

        let _ = stream.write(response.as_bytes()).unwrap();
    }

}
