// vim: sw=4 et filetype=rust

extern crate env_logger;
extern crate log;
extern crate num_cpus;

use anyhow::anyhow;
use anyhow::Result;
use env_logger::Env;
use indoc::formatdoc;
use log::{info, trace, warn};
use signal_hook::consts::{SIGINT, SIGQUIT};
use signal_hook::iterator::Signals;
use std::collections::HashMap;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::thread;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::time::Duration;


const BUFFER_SIZE: usize = 32;
const TIME_FORMAT_VERSION: usize = 2;
// const STREAM_READ_INTO_BUFFER_TIMEOUT: Duration = Duration::from_millis(500);
const STREAM_READ_INTO_BUFFER_TIMEOUT: Duration = Duration::from_secs(3600);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_contain_end_of_http_headers_full_example() {
        let array = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nBody";
        assert_eq!(bytes_contain_eoh(array).unwrap(), Some(41));
    }

    #[test]
    fn test_bytes_contain_end_of_http_headers_not() {
        let array: [u8; 32] = [0; 32];
        assert_eq!(bytes_contain_eoh(&array).unwrap(), None);
    }

    #[test]
    fn test_bytes_contain_end_of_http_headers_at_end() {
        let mut array: [u8; 32] = [0; 32];
        array[28..32].copy_from_slice(b"\r\n\r\n");
        assert_eq!(bytes_contain_eoh(&array).unwrap(), Some(28));
    }

    #[test]
    fn test_bytes_contain_end_of_http_headers_in_middle() {
        let mut array: [u8; 32] = [0; 32];
        array[9..13].copy_from_slice(b"\r\n\r\n");
        assert_eq!(bytes_contain_eoh(&array).unwrap(), Some(9));
    }

    #[test]
    fn test_bytes_contain_end_of_http_headers_multiple() {
        let mut array: [u8; 32] = [0; 32];
        array[28..32].copy_from_slice(b"\r\n\r\n");
        array[9..13].copy_from_slice(b"\r\n\r\n");
        assert_eq!(bytes_contain_eoh(&array).unwrap(), Some(9));
    }

    #[test]
    fn test_bytes_contain_end_of_http_headers_at_begin() {
        let mut array: [u8; 32] = [0; 32];
        array[0..4].copy_from_slice(b"\r\n\r\n");
        assert_eq!(bytes_contain_eoh(&array).unwrap(), Some(0));
    }

    #[test]
    fn test_bytes_contain_end_of_http_headers_partial() {
        let mut array: [u8; 32] = [0; 32];
        array[0..2].copy_from_slice(b"\r\n");
        assert_eq!(bytes_contain_eoh(&array).unwrap(), None);
    }
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
/// - `Some(usize)` - If the EOH marker is found, returns the index at which the marker begins.
/// - `None` - If the EOH marker is not found in the given byte slice.
///
/// # Panics
///
/// This function panics if the slice's length is less than 4 bytes, which is the length of the EOH marker.
///
/// # Examples
///
/// ```
/// let data = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nBody";
/// let index = bytes_contain_eoh(data);
/// assert_eq!(index, Some(41));
/// ```
///
/// # Notes
///
/// This function is optimized for searching small slices, typical of HTTP header sizes.
/// For very large slices, consider using more efficient string searching algorithms.
fn bytes_contain_eoh(content: &[u8]) -> Result<Option<usize>, String> {
    let target = b"\r\n\r\n";
    if content.len() < target.len() {
        return Err("Input slice is smaller than target sequence length".into());
    }

    Ok((0..=(content.len() - target.len())).find(|&i| content[i..i + target.len()] == *target))
}

#[must_use]
fn new_date_time_http_rfc() -> String {
    let now = time::OffsetDateTime::now_utc();
    let format = time::format_description::parse_borrowed::<TIME_FORMAT_VERSION>(
        "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT",
    )
    .expect("Error creating HTTP timestamp");
    now.format(&format).unwrap_or_default()
}

#[must_use]
fn compose_http_response_headers(content_len: usize, content_type: &str) -> String {
    let headers = formatdoc! {"
        date: {date}\r
        content-type: {content_type}\r
        content-length: {content_len}\r
        connection: close",
        date = new_date_time_http_rfc(),
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

async fn read_stream_into(buffer: &mut [u8], stream: &mut TcpStream) -> Result<usize> {
    let result_timeout =
        tokio::time::timeout(STREAM_READ_INTO_BUFFER_TIMEOUT, stream.read(buffer)).await;

    let result_read = match result_timeout {
        Ok(result_read) => result_read,
        Err(err) => {
            warn!("Read timed out: {err}");
            return Err(anyhow!(err));
        }
    };

    let bytes_read = match result_read {
        Ok(bytes_read) => bytes_read,
        Err(err) => {
            warn!("Worker: 🤷 Error while reading from remote, aborting connection. Here is the error we got: {err}");
            return Err(anyhow!(err));
        }
    };

    Ok(bytes_read)
}

async fn send_basic_status_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_message: &str,
) {
    let body = format!("{status_message}\n");
    let response_headers = compose_http_response_headers(body.len(), "text/plain; charset=utf-8");
    let response = compose_http_response(
        status_code,
        status_message,
        response_headers.as_str(),
        &body,
    );

    let _ = stream.write(response.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

async fn handle_connection(stream: &mut TcpStream) -> Result<()> {
    let request_header_limit_bytes = 4096;
    let mut request_headers: &[u8] = &[0u8];
    let request_body: Vec<u8>;
    let bytes_body_read: usize = 0;
    let mut request_data: Vec<u8> = Vec::with_capacity(BUFFER_SIZE);
    let request_body_byte_index_start;
    let mut buffer = [0; BUFFER_SIZE];
    let mut request_header_limit_bytes_exeeded = false;
    let mut request_smaller_then_4_bytes = false;
    let mut request_header_incomplete = false;

    loop {
        let Ok(bytes_read) = read_stream_into(&mut buffer, stream).await else {
            warn!("reading the stream failed, so we are going to bail out of this conneciton");
            return Ok(());
        };

        let buffer_trimmed = match buffer.get(..bytes_read) {
            Some(bytes) => bytes,
            None => {
                warn!("We could not trimm the input buffer. The buffer has a length of {BUFFER_SIZE} and we got {bytes_read} bytes.");
                return Ok(());
            }
        };

        // FIXME: headers might end within the current buffer, this is not checked
        if request_data.len() + buffer_trimmed.len() >= request_header_limit_bytes {
            request_header_limit_bytes_exeeded = true;
            break;
        }

        request_data.extend(buffer_trimmed);
        let maybe_eoh_byte_index = match bytes_contain_eoh(request_data.as_slice()) {
            Ok(result) => result,
            Err(_error) => {
                request_smaller_then_4_bytes = true;
                break;
            }
        };
        if let Some(eoh_byte_index) = maybe_eoh_byte_index {
            request_headers = request_data.get(..eoh_byte_index).unwrap();
            request_body_byte_index_start = eoh_byte_index + 4; // +4 to skip over the \r\n\r\n at the end
            break;
        }

        if bytes_read == 0 || bytes_read < BUFFER_SIZE {
            // since we haven't found the end-of-headers marker (\\r\\n\\r\\n)
            request_header_incomplete = true;
            break;
        }
    }

    if request_header_incomplete {
        warn!("Since we haven't found the \"\\r\\n\\r\\n\" marker but the remote indicated that they are done sending data we can conclude that the request must be incomplete because all http requests must contain the \"\\r\\n\\r\\n\" sequence at least once.");
        send_basic_status_response(stream, 400, "Bad Request").await;
        return Ok(());
    }

    if request_smaller_then_4_bytes {
        warn!("We received less then 4 bytes in total, which cant be a valid request. Aborting connection...");
        send_basic_status_response(stream, 400, "Bad Request").await;
        return Ok(());
    }

    if request_header_limit_bytes_exeeded {
        warn!("Request header size limit of {request_header_limit_bytes} has been exceeded. Aborting connection...");
        send_basic_status_response(stream, 400, "Bad Request").await;
        return Ok(());
    }

    //print!("{:02X?}\n", request_data);
    //io::stdout().flush().unwrap();

    // TODO: read request body and determined if the body sould also be fetched.

    let request_headers = String::from_utf8_lossy(request_headers);
    let request_body = String::new();

    trace!("Request headers:\n{request_headers}");
    let mut file = File::create("request_headers.txt").await.unwrap();
    file.write_all(request_headers.as_bytes()).await.unwrap();
    trace!("Request body:\n{request_body}");
    // let mut file = File::create("request_body.txt").unwrap();
    // file.write_all(request_body.as_bytes()).unwrap();

    if bytes_body_read >= 50_000_000 {
        let status_code = 413;
        let status_message = "Payload Too Large";
        let body = "413 - Payload Too Large\n";
        let response_headers =
            compose_http_response_headers(body.len(), "text/plain; charset=utf-8");
        let response =
            compose_http_response(status_code, status_message, response_headers.as_str(), body);

        let _ = stream.write(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        return Ok(());
    }

    //debug!(
    //    "Worker: {request}",
    //    request = request_headers.lines().take(1).collect::<String>()
    //);

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

    let response_headers = compose_http_response_headers(body.len(), content_type);
    let response = compose_http_response(
        status_code,
        status_message,
        response_headers.as_str(),
        body.as_str(),
    );

    let _ = stream.write(response.as_bytes()).await.unwrap();
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .format_target(false)
        .format_indent(Some("[0000-00-00T00:00:00.000Z INFO ] ".len()))
        .init();

    let _ = thread::Builder::new()
        .name("ProcessSignalHandler".into())
        .spawn(move || {
            let mut signals = Signals::new([SIGINT, SIGQUIT]).unwrap();
            for sig in signals.forever() {
                info!("Received process signal {sig:?}");
                process::exit(0);
            }
        });

    let worker_count = num_cpus::get();

    let bind_addr = "127.0.0.1:8000";
    info!("listening on {bind_addr}");
    let listener = TcpListener::bind(bind_addr).await?;
    let semaphore = Arc::new(Semaphore::new(worker_count));

    loop {
        let (mut stream, socket_addr) = listener.accept().await?;
        let permit = Arc::clone(&semaphore).acquire_owned().await.unwrap();

        trace!("Wohoo, we got a new connection from {socket_addr} 🙌");

        tokio::spawn(async move {
            handle_connection(&mut stream).await.unwrap();
            trace!("We are done with a request from {socket_addr}");
            drop(permit);
        });
    }
}
