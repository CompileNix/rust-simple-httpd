use crate::config::Config;
use crate::Level;
use crate::{debug, trace, verb, warn, error};
use indoc::formatdoc;
use std::any::Any;
use std::net::TcpStream;
use std::thread;
use std::sync::{Arc, mpsc, Mutex};
use std::collections::HashMap;
use std::io::{Read, Write};
use crate::tcp::{ConnectionHandler, ConnectionHandlerMessage};

#[cfg(feature = "log-trace")]
use crate::util::highlighted_hex_vec;

#[derive(Debug)]
pub struct Server {
    connection_handler: ConnectionHandler,
}

impl Server {
    pub fn new(
        config: &Config,
        receiver: Arc<Mutex<mpsc::Receiver<ConnectionHandlerMessage>>>,
        sender: mpsc::Sender<ConnectionHandlerMessage>,
    ) -> Server {
        let config = config.clone();
        let cfg = &config;

        let connection_handler = ConnectionHandler::new(
            receiver,
            config.clone(),
        );
        let listener = connection_handler.bind();

        trace!(cfg, "Start TcpListener thread");
        let listener_config = config.clone();
        let _ = thread::Builder::new()
            .name("TcpListener".into())
            .spawn(move || {
            let cfg = &listener_config;
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        if let Err(err) = sender.send(ConnectionHandlerMessage::NewConnection(stream)) {
                            error!(cfg, "Failed to send new connection to connection handler thread, shutting down listener: {err}");
                            break;
                        }
                    },
                    Err(err) => {
                        warn!(cfg, "incoming connection error: {err}");
                    }
                }
            }
            // NOTE: unreachable under normal circumstances. Graceful shutdown
            // can't be passed into TcpListener thread, because it's usually
            // waiting on `incoming()` for a new connection.
            trace!(cfg, "TcpListener thread shutdown");
        });

        Server { connection_handler }
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
    pub fn bytes_contain_eoh(content: &[u8]) -> Option<usize> {
        let eoh_sequence = b"\r\n\r\n"; // 0x0d, 0x0a, 0x0d, 0x0a

        if content.len() < eoh_sequence.len() {
            return None;
        }

        // Return the position found, or None if not found
        content
            .windows(eoh_sequence.len())
            .position(|window| window == eoh_sequence)
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
            date = Server::new_date_time_http_rfc(),
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
    ) -> Result<usize, std::io::Error> {
        let cfg = config;

        // Set stream read and write timeouts
        if let Err(err) = stream.set_read_timeout(Some(cfg.buffer_read_client_timeout)) {
            warn!(cfg, "[{worker_id}]: set_read_timeout call failed: {err}");
            return Err(err);
        }
        if let Err(err) = stream.set_write_timeout(Some(cfg.buffer_write_client_timeout)) {
            warn!(cfg, "[{worker_id}]: set_write_timeout call failed: {err}");
            return Err(err);
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
                return Err(err);
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

    /// Parse HTTP request headers into a map for easy access.
    ///
    /// This function takes a slice of bytes representing the raw HTTP request headers,
    /// converts it to a `String`, splits it by newline characters, and then parses each line
    /// as a key-value pair. The result is stored in a `HashMap` where keys are header names
    /// and values are header values.
    ///
    /// # Arguments
    ///
    /// * `request_headers_bytes` - A slice of bytes (`&[u8]`) containing the raw HTTP request headers.
    ///
    /// # Returns
    ///
    /// Returns a `HashMap<String, String>` where each key is a header name and each value is
    /// the corresponding header value. If a header appears multiple times in the input,
    /// only the last occurrence will be kept in the map.
    pub fn parse_request_headers(request_headers_bytes: &[u8], headers_map: &mut HashMap<String, String>) {
        let request_headers_str = String::from_utf8_lossy(request_headers_bytes);
        for line in request_headers_str.lines() {
            if let Some(key_value) = line.split_once(':') {
                let header_key = key_value.0.trim().to_lowercase();
                let header_value = key_value.1.trim().to_lowercase();
                headers_map.insert(header_key, header_value);
            }
        }
    }

    // TODO: continue work
    pub fn handle_connection(stream: &mut TcpStream, worker_id: usize, config: &Config) {
        let cfg = config;

        trace!(cfg, "[{worker_id}]: handle request from client {}", stream.peer_addr().expect("Failed to get peer address"));

        let mut request_headers = vec![0; config.buffer_client_receive_size];
        let bytes_body_read: usize = 0;
        let mut request_data = vec![];
        let mut request_body_byte_index_start = 0;
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

            // copy trimmed buffer contents to request_data
            request_data.extend(buffer_trimmed);

            // reset buffer
            buffer.fill(0);

            // check if the client is done with sending request headers
            if let Some(eoh_byte_index) = Self::bytes_contain_eoh(request_data.as_slice()) {
                trace!(cfg, "[{worker_id}]: EOH sequence found at {eoh_byte_index}");
                if let Some(headers) = request_data.get(..eoh_byte_index) {
                    request_headers = headers.to_vec();
                } else {
                    warn!(cfg, "[{worker_id}]: We could not get the request headers from the buffer. The buffer has a length of {buffer_size} and we got {bytes_read} bytes.", buffer_size = config.buffer_client_receive_size);
                    return;
                }
                request_body_byte_index_start = eoh_byte_index + 4; // +4 to skip over the \r\n\r\n at the end
                trace!(cfg, "[{worker_id}]: request_body_byte_index_start: {request_body_byte_index_start}");
                break;
            }

            // since we haven't found the end-of-headers marker (\\r\\n\\r\\n), the request can't be valid / complete
            if bytes_read == 0 || bytes_read < config.buffer_client_receive_size {
                request_header_incomplete = true;
                break;
            }
        }

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

        let request_headers = String::from_utf8_lossy(&request_headers);
        debug!(cfg, "[{worker_id}]: final request headers raw:\n{request_headers}");
        // let request_body = String::new();

        // parse http request headers, without a mem copy into a simple data structure which enables convenient access of individual header keys and values
        let mut request_headers_map = HashMap::new();

        Server::parse_request_headers(request_headers.as_bytes(), &mut request_headers_map);

        for (header_key, header_value) in &request_headers_map {
            trace!(cfg, "[{worker_id}] final request headers parsed: {header_key}: {header_value}");
        }

        // Check for chunked transfer encoding
        if request_headers_map.get("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
            warn!(cfg, "[{worker_id}]: Chunked transfer encoding is not supported yet");
            Self::send_basic_status_response(stream, 501, "Not Implemented");
            return;
        }

        // Read a request body if present
        let content_length_header = request_headers_map
            .get("content-length")
            .and_then(|len| len.parse::<usize>().ok());

        let request_body: Vec<u8> = if let Some(http_request_content_length) = content_length_header {
            debug!(cfg, "[{worker_id}]: Content-Length header found, reading {http_request_content_length} bytes of body data");

            // First, get any body data that was already read into request_data
            let mut body = if request_data.len() > request_body_byte_index_start {
                debug!(cfg, "[{worker_id}]: receive beginning of body data ({} bytes) from previous reads of request headers into body", request_data.len() - request_body_byte_index_start);
                trace!(cfg, "[{worker_id}]: data raw: {}", highlighted_hex_vec(&request_data[request_body_byte_index_start..], 0, cfg));
                trace!(cfg, "[{worker_id}]: data string: {}", String::from_utf8_lossy(&request_data[request_body_byte_index_start..]));
                request_data.get(request_body_byte_index_start..).unwrap_or_default().to_vec()
            } else {
                Vec::new()
            };

            // read remaining body data from the stream until we have either read the requested
            // number of bytes or the stream has been closed
            while body.len() < http_request_content_length {
                let remaining = http_request_content_length - body.len();
                let to_read = remaining.min(config.buffer_client_receive_size);
                let buffer_slice = buffer.get_mut(..to_read)
                    .expect("Unable to get u8 vec slice for receive buffer");

                let Ok(bytes_read) = Self::read_stream_into(buffer_slice, stream, worker_id, cfg) else {
                    warn!(cfg, "[{worker_id}]: Failed to read request body data");
                    return;
                };

                if bytes_read == 0 {
                    warn!(cfg, "[{worker_id}]: Connection closed before receiving complete body");
                    return;
                }

                trace!(cfg, "[{worker_id}]: body data chunk raw: {}", highlighted_hex_vec(&buffer[..bytes_read], body.len(), cfg));
                trace!(cfg, "[{worker_id}]: body data chunk string: {}", String::from_utf8_lossy(&buffer[..bytes_read]));

                body.extend_from_slice(buffer.get(..bytes_read).unwrap_or_default());
            }

            debug!(cfg, "[{worker_id}]: Finished reading request body, total bytes read: {}", body.len());
            debug!(cfg, "[{worker_id}]: final body data raw: {}", highlighted_hex_vec(&body, 0, cfg));
            debug!(cfg, "[{worker_id}]: final body data string: {}", String::from_utf8_lossy(&body));
            body
        } else {
            debug!(cfg, "[{worker_id}]: No Content-Length header found, assuming no body");
            Vec::new()
        };

        // let mut file = File::create("request_headers.txt").unwrap();
        // file.write_all(request_headers.as_bytes()).unwrap();
        //verb!(cfg, "[{worker_id}]: Request body:\n{request_body}");
        // let mut file = File::create("request_body.txt").unwrap();
        // file.write_all(request_body.as_bytes()).unwrap();

        if bytes_body_read >= 50_000_000 {
            warn!(cfg, "[{worker_id}]: Payload too large {bytes_body_read} bytes > 50_000_000 bytes");
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
        let content_type = "text/html; charset=utf-8";

        let response_headers = Self::compose_http_response_headers(body.len(), content_type);
        let response = Self::compose_http_response(
            status_code,
            status_message,
            response_headers.as_str(),
            body.as_str(),
        );

        verb!(cfg, "[{worker_id}]: Send HTTP {status_code} {status_message} to client at {}", stream.peer_addr().expect("Failed to get peer address"));
        let _ = stream.write(response.as_bytes());
        stream.flush().ok();
    }
}
