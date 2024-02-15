use std::collections::HashMap;
use std::path::Path;
use anyhow::anyhow;
use indoc::formatdoc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use crate::config::Config;

#[derive(Clone, Copy, Debug, Hash, Default)]
pub struct HttpServer {
  pub config: Config,
  pub log: crate::log::Logger,
}

impl HttpServer {
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

  async fn read_stream_into(&self, buffer: &mut [u8], stream: &mut TcpStream) -> anyhow::Result<usize> {
    let result_timeout =
      tokio::time::timeout(self.config.buffer_read_client_timeout, stream.read(buffer)).await;

    let result_read = match result_timeout {
      Ok(result_read) => result_read,
      Err(err) => {
        self.log.warn(&format!("Read timed out: {err}"));
        return Err(anyhow!(err));
      }
    };

    let bytes_read = match result_read {
      Ok(bytes_read) => bytes_read,
      Err(err) => {
        self.log.warn(&format!("Worker: 🤷 Error while reading from remote, aborting connection. Here is the error we got: {err}"));
        return Err(anyhow!(err));
      }
    };

    self.log.trace(&format!(
      "stream.read(buffer({})): returned {} bytes",
      buffer.len(),
      bytes_read
    ));

    Ok(bytes_read)
  }

  async fn send_basic_status_response(
    &self,
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

    let _ = stream.write(response.as_bytes()).await;
    stream.flush().await.ok();
  }

  pub async fn handle_connection(&self, stream: &mut TcpStream) {
    let log = crate::log::Logger {
      level: self.config.log_level,
    };
    let mut request_headers = vec![0; self.config.buffer_client_receive_size];
    let request_body: Vec<u8>;
    let bytes_body_read: usize = 0;
    let mut request_data = vec![0; self.config.buffer_client_receive_size];
    let request_body_byte_index_start;
    let mut buffer = vec![0; self.config.buffer_client_receive_size];
    let mut request_header_limit_bytes_exceeded = false;
    let mut request_header_incomplete = false;

    // Reading request headers
    log.debug("begin reading stream into buffer until EOH");
    log.trace("EOH sequence is \"\\r\\n\\r\\n\" (0x0d, 0x0a, 0x0d, 0x0a)");
    loop {
      // Read into buffer
      let Ok(bytes_read) = self.read_stream_into(&mut buffer, stream).await else {
        log.warn(
          "reading the stream failed, so we are going to bail out of this connection",
        );
        return;
      };

      // EOH marker is: 0x0d, 0x0a, 0x0d, 0x0a
      log.trace(&format!("buffer data: {:02x?}", &buffer));

      // Trim null bytes, in case the buffer wasn't filled
      let Some(buffer_trimmed) = buffer.get(..bytes_read) else {
        log.warn(&format!("We could not trim the input buffer. The buffer has a length of {buffer_size} and we got {bytes_read} bytes.", buffer_size = self.config.buffer_client_receive_size));
        return;
      };

      // Check if REQUEST_HEADER_LIMIT_BYTES has been exceeded
      // FIXME: headers might end within the current buffer, this is not checked
      if request_data.len() + buffer_trimmed.len() >= self.config.request_header_limit_bytes {
        request_header_limit_bytes_exceeded = true;
        break;
      }

      // append buffer to request_data and check if the client is done with sending request headers
      request_data.extend(buffer_trimmed);
      if let Some(eoh_byte_index) = Self::bytes_contain_eoh(request_data.as_slice()) {
        log.trace(&format!("EOH sequence found at {eoh_byte_index}"));
        request_headers = request_data.get(..eoh_byte_index).unwrap().to_vec();
        request_body_byte_index_start = eoh_byte_index + 4; // +4 to skip over the \r\n\r\n at the end
        break;
      }

      // since we haven't found the end-of-headers marker (\\r\\n\\r\\n), the request can't be valid / complete
      if bytes_read == 0 || bytes_read < self.config.buffer_client_receive_size {
        request_header_incomplete = true;
        break;
      }
    }
    buffer.fill(0);

    if request_header_incomplete {
      log.warn("Since we haven't found the \"\\r\\n\\r\\n\" marker but the remote indicated that they are done sending data we can conclude that the request must be incomplete because all http requests must contain the \"\\r\\n\\r\\n\" sequence at least once.");
      self.send_basic_status_response(stream, 400, "Bad Request")
        .await;
      return;
    }

    if request_header_limit_bytes_exceeded {
      log.warn(&format!("Request header size limit of {request_header_limit_bytes} has been exceeded. Aborting connection...", request_header_limit_bytes = self.config.request_header_limit_bytes));
      self.send_basic_status_response(stream, 400, "Bad Request")
        .await;
      return;
    }

    //print!("{:02X?}\n", request_data);
    //io::stdout().flush().unwrap();

    // TODO: read request body and determined if the body should also be fetched.

    let request_headers = String::from_utf8_lossy(&request_headers);
    let request_body = String::new();

    log.debug(&format!("Request headers:\n{request_headers}"));
    let mut file = File::create("request_headers.txt").await.unwrap();
    file.write_all(request_headers.as_bytes()).await.unwrap();
    log.trace(&format!("Request body:\n{request_body}"));
    // let mut file = File::create("request_body.txt").unwrap();
    // file.write_all(request_body.as_bytes()).unwrap();

    if bytes_body_read >= 50_000_000 {
      self.send_basic_status_response(stream, 413, "Payload To Large")
        .await;
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

    let mut files = HashMap::new();
    if Path::new("index.html").exists() {
      let mut contents = String::new();
      tokio::fs::File::open("index.html")
        .await
        .expect(r#"Unable to open file "index.html" for reading"#)
        .read_to_string(&mut contents)
        .await
        .expect(r#"error while reading "index.html" into a `String` type"#);
      files.insert(String::from("/index.html"), contents);
    }

    let mut status_code = 404;
    let mut status_message = "Not Found";
    let mut body = String::from("Not Found\n");
    let mut content_type = "text/plain; charset=utf-8";
    if let Some(x) = files.get("/index.html") {
      body = x.into();
      status_code = 200;
      status_message = "OK";
      content_type = "text/html; charset=utf-8";
    }

    let response_headers = Self::compose_http_response_headers(body.len(), content_type);
    let response = Self::compose_http_response(
      status_code,
      status_message,
      response_headers.as_str(),
      body.as_str(),
    );

    let _ = stream.write(response.as_bytes()).await.unwrap();
  }
}
