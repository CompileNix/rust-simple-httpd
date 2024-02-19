use crate::http_server::HttpServer;
use crate::util;
use std::fmt;
use std::fmt::Formatter;

// fn new_http_server() -> HttpServer {
//     HttpServer { config: Config::default(), log: Logger { level: Level::Trace } }
// }

#[test]
fn bytes_contain_end_of_http_headers_full_example() {
    let array = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nBody";
    assert_eq!(HttpServer::bytes_contain_eoh(array), Some(41));
}

#[test]
fn bytes_contain_end_of_http_headers_not() {
    let array: [u8; 32] = [0; 32];
    assert_eq!(HttpServer::bytes_contain_eoh(&array), None);
}

#[test]
fn bytes_contain_end_of_http_headers_at_end() {
    let mut array: [u8; 32] = [0; 32];
    array[28..32].copy_from_slice(b"\r\n\r\n");
    assert_eq!(HttpServer::bytes_contain_eoh(&array), Some(28));
}

#[test]
fn bytes_contain_end_of_http_headers_in_middle() {
    let mut array: [u8; 32] = [0; 32];
    array[9..13].copy_from_slice(b"\r\n\r\n");
    assert_eq!(HttpServer::bytes_contain_eoh(&array), Some(9));
}

#[test]
fn bytes_contain_end_of_http_headers_multiple() {
    let mut array: [u8; 32] = [0; 32];
    array[28..32].copy_from_slice(b"\r\n\r\n");
    array[9..13].copy_from_slice(b"\r\n\r\n");
    assert_eq!(HttpServer::bytes_contain_eoh(&array), Some(9));
}

#[test]
fn bytes_contain_end_of_http_headers_at_begin() {
    let mut array: [u8; 32] = [0; 32];
    array[0..4].copy_from_slice(b"\r\n\r\n");
    assert_eq!(HttpServer::bytes_contain_eoh(&array), Some(0));
}

#[test]
fn bytes_contain_end_of_http_headers_partial() {
    let mut array: [u8; 32] = [0; 32];
    array[0..2].copy_from_slice(b"\r\n");
    assert_eq!(HttpServer::bytes_contain_eoh(&array), None);
}

#[test]
fn bytes_contain_end_of_http_headers_too_small() {
    let array: [u8; 3] = [0; 3];
    assert_eq!(HttpServer::bytes_contain_eoh(&array), None);
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
