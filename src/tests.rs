#![allow(clippy::unwrap_used)]

use std::fmt;
use std::fmt::Formatter;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

use rand::Rng;
use reqwest::StatusCode;

use crate::http::Server;
use crate::log;
use crate::tcp;
use crate::util;
use crate::Config;

fn get_random_port() -> u16 {
    let mut rng = rand::rng();
    let range = 20000..=40000;
    rng.random_range(range)
}

fn get_random_local_bind_addr() -> String {
    format!("127.0.0.1:{}", get_random_port())
}

#[test]
fn bytes_contain_end_of_http_headers_full_example() {
    let array = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nBody";
    assert_eq!(Server::bytes_contain_eoh(array), Some(41));
}

#[test]
fn bytes_contain_end_of_http_headers_not() {
    let array: [u8; 32] = [0; 32];
    assert_eq!(Server::bytes_contain_eoh(&array), None);
}

#[test]
fn bytes_contain_end_of_http_headers_at_end() {
    let mut array: [u8; 32] = [0; 32];
    array[28..32].copy_from_slice(b"\r\n\r\n");
    assert_eq!(Server::bytes_contain_eoh(&array), Some(28));
}

#[test]
fn bytes_contain_end_of_http_headers_in_middle() {
    let mut array: [u8; 32] = [0; 32];
    array[9..13].copy_from_slice(b"\r\n\r\n");
    assert_eq!(Server::bytes_contain_eoh(&array), Some(9));
}

#[test]
fn bytes_contain_end_of_http_headers_multiple() {
    let mut array: [u8; 32] = [0; 32];
    array[28..32].copy_from_slice(b"\r\n\r\n");
    array[9..13].copy_from_slice(b"\r\n\r\n");
    assert_eq!(Server::bytes_contain_eoh(&array), Some(9));
}

#[test]
fn bytes_contain_end_of_http_headers_at_begin() {
    let mut array: [u8; 32] = [0; 32];
    array[0..4].copy_from_slice(b"\r\n\r\n");
    assert_eq!(Server::bytes_contain_eoh(&array), Some(0));
}

#[test]
fn bytes_contain_end_of_http_headers_partial() {
    let mut array: [u8; 32] = [0; 32];
    array[0..2].copy_from_slice(b"\r\n");
    assert_eq!(Server::bytes_contain_eoh(&array), None);
}

#[test]
fn bytes_contain_end_of_http_headers_too_small() {
    let array: [u8; 3] = [0; 3];
    assert_eq!(Server::bytes_contain_eoh(&array), None);
}

#[derive(Debug)]
struct TestFormatWithOptions {
    value: String,
}

impl fmt::Display for TestFormatWithOptions {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        util::format_with_options(&self.value, f)
    }
}

#[test]
fn format_with_options_basic_no_width() {
    let test = TestFormatWithOptions {
        value: String::from("Test"),
    };
    assert_eq!(format!("{test}"), "Test");
}

#[test]
fn format_with_options_width_default_alignment() {
    let test = TestFormatWithOptions {
        value: String::from("Test"),
    };
    assert_eq!(format!("{test:8}"), "Test    ");
}

#[test]
fn format_with_options_width_align_left() {
    let test = TestFormatWithOptions {
        value: String::from("Test"),
    };
    assert_eq!(format!("{test:<8}"), "Test    ");
}

#[test]
fn format_with_options_width_align_right() {
    let test = TestFormatWithOptions {
        value: String::from("Test"),
    };
    assert_eq!(format!("{test:>8}"), "    Test");
}

#[test]
fn format_with_options_width_align_center() {
    let test = TestFormatWithOptions {
        value: String::from("Test"),
    };
    assert_eq!(format!("{test:^8}"), "  Test  ");
}

#[test]
#[cfg(feature = "log-trace")]
fn count_num_digits() {
    assert_eq!(util::num_digits(0), 1);
    assert_eq!(util::num_digits(5), 1);
    assert_eq!(util::num_digits(42), 2);
    assert_eq!(util::num_digits(420), 3);
    assert_eq!(util::num_digits(1000), 4);
    assert_eq!(util::num_digits(12345), 5);
    assert_eq!(util::num_digits(123_456), 6);
    assert_eq!(util::num_digits(1_234_567), 7);
    assert_eq!(util::num_digits(12_345_678), 8);
    assert_eq!(util::num_digits(123_456_789), 9);
}

#[test]
fn available_parallelism_capped() {
    let available = std::thread::available_parallelism().expect("Failed to get available_parallelism from std::thread").get();

    assert_eq!(util::available_parallelism_capped_at(0), available);
    assert_eq!(util::available_parallelism_capped_at(1), 1);
    assert_eq!(util::available_parallelism_capped_at(2), 2);
    assert_eq!(util::available_parallelism_capped_at(available), available);
    assert_eq!(util::available_parallelism_capped_at(999_999_999_999), available);
}

#[test]
#[cfg(all(feature = "log-trace", feature = "humantime", feature = "color"))]
fn highlighted_hex_vec() {
    // highlighted_hex_vec(&buffer, request_data.len(), cfg)

    let mut cfg = Config::default_from_env().expect("Failed to get default config from env");
    if !util::is_colored_output_avail(&cfg) {
        cfg.colored_output = false;
    }

    cfg.colored_output = false;
    let buffer = vec![118, 247, 158, 120, 199, 236, 45, 23, 182, 121, 6, 13, 215, 239, 222, 18, 25, 39, 83, 10, 72, 45, 179, 205, 199, 226, 79, 249, 57, 36, 219, 193];
    assert_eq!(util::highlighted_hex_vec(&buffer, 0, &cfg), "
                                    0 = 76 f7 9e 78 c7 ec 2d 17 
                                    8 = b6 79 06 \\r d7 ef de 12 
                                   16 = 19 27 53 \\n 48 2d b3 cd 
                                   24 = c7 e2 4f f9 39 24 db c1");
    let buffer = vec![226, 222, 252, 182, 195, 97, 238, 236, 55, 247, 14, 72, 105, 44, 253, 105, 119, 29, 133, 156, 96, 207, 198, 172, 241, 82, 33, 32, 186, 164, 198, 244];
    assert_eq!(util::highlighted_hex_vec(&buffer, buffer.len(), &cfg), "
                                   32 = e2 de fc b6 c3 61 ee ec 
                                   40 = 37 f7 0e 48 69 2c fd 69 
                                   48 = 77 1d 85 9c 60 cf c6 ac 
                                   56 = f1 52 21 20 ba a4 c6 f4");

    cfg.colored_output = true;
    cfg.colored_output_forced = true;
    let buffer = vec![118, 247, 158, 120, 199, 236, 45, 23, 182, 121, 6, 13, 215, 239, 222, 18, 25, 39, 83, 10, 72, 45, 179, 205, 199, 226, 79, 249, 57, 36, 219, 193];
    assert_eq!(util::highlighted_hex_vec(&buffer, 0, &cfg), "
                                    0 = 76 f7 9e 78 c7 ec 2d 17 
                                    8 = b6 79 06 \u{1b}[33m\\r\u{1b}[0m d7 ef de 12 
                                   16 = 19 27 53 \u{1b}[33m\\n\u{1b}[0m 48 2d b3 cd 
                                   24 = c7 e2 4f f9 39 24 db c1");
    let buffer = vec![226, 222, 252, 182, 195, 97, 238, 236, 55, 247, 14, 72, 105, 44, 253, 105, 119, 29, 133, 156, 96, 207, 198, 172, 241, 82, 33, 32, 186, 164, 198, 244];
    assert_eq!(util::highlighted_hex_vec(&buffer, buffer.len(), &cfg), "
                                   32 = e2 de fc b6 c3 61 ee ec 
                                   40 = 37 f7 0e 48 69 2c fd 69 
                                   48 = 77 1d 85 9c 60 cf c6 ac 
                                   56 = f1 52 21 20 ba a4 c6 f4");

    cfg.colored_output = false;
    cfg.colored_output_forced = true;
    let buffer = vec![118, 247, 158, 120, 199, 236, 45, 23, 182, 121, 6, 13, 215, 239, 222, 18, 25, 39, 83, 10, 72, 45, 179, 205, 199, 226, 79, 249, 57, 36, 219, 193];
    assert_eq!(util::highlighted_hex_vec(&buffer, 0, &cfg), "
                                    0 = 76 f7 9e 78 c7 ec 2d 17 
                                    8 = b6 79 06 \u{1b}[33m\\r\u{1b}[0m d7 ef de 12 
                                   16 = 19 27 53 \u{1b}[33m\\n\u{1b}[0m 48 2d b3 cd 
                                   24 = c7 e2 4f f9 39 24 db c1");
    let buffer = vec![226, 222, 252, 182, 195, 97, 238, 236, 55, 247, 14, 72, 105, 44, 253, 105, 119, 29, 133, 156, 96, 207, 198, 172, 241, 82, 33, 32, 186, 164, 198, 244];
    assert_eq!(util::highlighted_hex_vec(&buffer, buffer.len(), &cfg), "
                                   32 = e2 de fc b6 c3 61 ee ec 
                                   40 = 37 f7 0e 48 69 2c fd 69 
                                   48 = 77 1d 85 9c 60 cf c6 ac 
                                   56 = f1 52 21 20 ba a4 c6 f4");

    cfg.colored_output = false;
    cfg.colored_output_forced = false;
    let buffer = vec![118, 247, 158, 120, 199, 236, 45, 23, 182, 121, 6, 13, 215, 239, 222, 18, 25, 39, 83, 10, 72, 45, 179, 205, 199, 226, 79, 249, 57, 36, 219, 193];
    assert_eq!(util::highlighted_hex_vec(&buffer, 0, &cfg), "
                                    0 = 76 f7 9e 78 c7 ec 2d 17 
                                    8 = b6 79 06 \\r d7 ef de 12 
                                   16 = 19 27 53 \\n 48 2d b3 cd 
                                   24 = c7 e2 4f f9 39 24 db c1");
    let buffer = vec![226, 222, 252, 182, 195, 97, 238, 236, 55, 247, 14, 72, 105, 44, 253, 105, 119, 29, 133, 156, 96, 207, 198, 172, 241, 82, 33, 32, 186, 164, 198, 244];
    assert_eq!(util::highlighted_hex_vec(&buffer, buffer.len(), &cfg), "
                                   32 = e2 de fc b6 c3 61 ee ec 
                                   40 = 37 f7 0e 48 69 2c fd 69 
                                   48 = 77 1d 85 9c 60 cf c6 ac 
                                   56 = f1 52 21 20 ba a4 c6 f4");
}

#[tokio::test]
async fn http_server_simple_valid_request() {
    let mut cfg = Config::default_from_env().expect("Failed to get default config from env");
    cfg.log_level = log::Level::Trace;
    cfg.bind_addr = get_random_local_bind_addr();
    let cfg = &cfg;

    let (sender, receiver) = mpsc::channel::<tcp::ConnectionHandlerMessage>();
    let receiver = Arc::new(Mutex::new(receiver));
    let server = Server::new(cfg, receiver, sender.clone());

    // Do the testing
    let url = format!("http://{}/", cfg.bind_addr);
    let response = reqwest::get(url).await;

    assert!(response.is_ok());
    let response = response.ok();

    assert!(response.is_some());
    let response = response.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("server").unwrap(), "rust-simple-httpd");
    assert_eq!(response.headers().get("content-type").unwrap(), "text/html; charset=utf-8");
    assert_eq!(response.headers().get("connection").unwrap(), "close");
    assert_eq!(response.text().await.unwrap(), "<!DOCTYPE html>\n<html lang=\"en\">\n\n<head>\n    <meta charset=\"utf-8\">\n    <title>Hello!</title>\n</head>\n\n<body>\n    <h1>Hello!</h1>\n    <p>Hi from Rust</p>\n</body>\n\n</html>\n");

    sender.send(tcp::ConnectionHandlerMessage::Shutdown).unwrap_or_default();
    let _ = server.serve();
}

#[test]
fn parse_duration_valid() {
    assert!(crate::config::parse_duration("1h 30m").is_ok());

    let parsed = crate::config::parse_duration("");
    assert!(parsed.is_ok());
    let duration = parsed.unwrap();
    assert_eq!(duration, std::time::Duration::new(0, 0));

    let parsed = crate::config::parse_duration("0");
    assert!(parsed.is_ok());
    let duration = parsed.unwrap();
    assert_eq!(duration, std::time::Duration::new(0, 0));

    let parsed = crate::config::parse_duration("10");
    assert!(parsed.is_ok());
    let duration = parsed.unwrap();
    assert_eq!(duration, std::time::Duration::new(10, 0));

    let parsed = crate::config::parse_duration("1000000000");
    assert!(parsed.is_ok());
    let duration = parsed.unwrap();
    assert_eq!(duration, std::time::Duration::new(1_000_000_000, 0));

    let parsed = crate::config::parse_duration("1h 30m");
    assert!(parsed.is_ok());
    let duration = parsed.unwrap();
    assert_eq!(duration, std::time::Duration::new(5400, 0));

    let parsed = crate::config::parse_duration("26h 30m 428s");
    assert!(parsed.is_ok());
    let duration = parsed.unwrap();
    assert_eq!(duration, std::time::Duration::new(95_828, 0));
}

#[test]
fn parse_duration_should_fail() {
    let result = crate::config::parse_duration("1g");
    assert!(matches!(result.clone().unwrap_err(), crate::config::ConfigError::Int(_)));
    if let Err(crate::config::ConfigError::Int(e)) = result {
        assert_eq!(e.to_string(), "invalid digit found in string");
    } else {
        panic!("Expected Err(crate::config::ConfigError::Int), got {result:?}");
    }

    let result = crate::config::parse_duration("-1");
    assert!(matches!(result.clone().unwrap_err(), crate::config::ConfigError::Int(_)));
    if let Err(crate::config::ConfigError::Int(e)) = result {
        assert_eq!(e.to_string(), "invalid digit found in string");
    } else {
        panic!("Expected Err(crate::config::ConfigError::Int), got {result:?}");
    }

    let result = crate::config::parse_duration("k");
    assert!(matches!(result.clone().unwrap_err(), crate::config::ConfigError::Int(_)));
    if let Err(crate::config::ConfigError::Int(e)) = result {
        assert_eq!(e.to_string(), "invalid digit found in string");
    } else {
        panic!("Expected Err(crate::config::ConfigError::Int), got {result:?}");
    }

    let result = crate::config::parse_duration("a");
    assert!(matches!(result.clone().unwrap_err(), crate::config::ConfigError::Int(_)));
    if let Err(crate::config::ConfigError::Int(e)) = result {
        assert_eq!(e.to_string(), "invalid digit found in string");
    } else {
        panic!("Expected Err(crate::config::ConfigError::Int), got {result:?}");
    }

    let result = crate::config::parse_duration("0x1");
    assert!(matches!(result.clone().unwrap_err(), crate::config::ConfigError::Int(_)));
    if let Err(crate::config::ConfigError::Int(e)) = result {
        assert_eq!(e.to_string(), "invalid digit found in string");
    } else {
        panic!("Expected Err(crate::config::ConfigError::Int), got {result:?}");
    }

    let result = crate::config::parse_duration("0e-9");
    assert!(matches!(result.clone().unwrap_err(), crate::config::ConfigError::Int(_)));
    if let Err(crate::config::ConfigError::Int(e)) = result {
        assert_eq!(e.to_string(), "invalid digit found in string");
    } else {
        panic!("Expected Err(crate::config::ConfigError::Int), got {result:?}");
    }
}
