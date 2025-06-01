# 🦀 rust-simple-httpd 🦀

A lightweight, educational HTTP server implementation written in Rust. This project serves as a learning playground for understanding HTTP protocol implementation, Rust systems programming, and concurrent server architecture.

## Overview

`rust-simple-httpd` is a from-scratch HTTP/1.1 server implementation that demonstrates core concepts of web server development in Rust. It features a multi-threaded architecture with worker pools, configurable logging, signal handling, and performance profiling capabilities.

**⚠️ Educational Purpose**: This is a learning project and playground for exploring Rust concepts. It's not intended for production use.

## Features

- **HTTP/1.1 Protocol Support**: Basic HTTP request/response handling
- **Multi-threaded Architecture**: Worker pool for handling concurrent connections
- **Configurable Logging**: Multiple log levels with optional colored output
- **Signal Handling**: Graceful shutdown on SIGINT/SIGQUIT
- **Performance Profiling**: Built-in flamegraph generation using `pprof`
- **Environment Configuration**: Configuration via environment variables
- **Static File Serving**: Basic static file serving capabilities
- **Connection Management**: TCP connection handling with timeouts
- **Comprehensive Testing**: Unit and integration tests included

## Architecture

The server is built with a modular architecture:

- **Main Thread**: Handles initialization, configuration, and signal processing
- **TCP Listener Thread**: Accepts incoming connections
- **Worker Pool**: Processes HTTP requests concurrently
- **Connection Handler**: Manages connection lifecycle and message passing

## Getting Started

### Prerequisites

- Rust 2024 edition (latest stable recommended)
- Cargo package manager

### Building

```bash
git clone https://git.compilenix.org/CompileNix/rust-simple-httpd
cd rust-simple-httpd

cargo build

# Build with optimizations
cargo build --release
```

### Running

```bash
# Run with default configuration
cargo run

# Run with custom bind address
BIND_ADDR=0.0.0.0:3000 cargo run

# Run with debug logging
RUST_LOG=debug cargo run
```

The server will start on `127.0.0.1:8000` by default and serve a simple "Hello from Rust" HTML page.

## Configuration

The server can be configured using environment variables:

| Variable                      | Default          | Description                                 |
|-------------------------------|------------------|---------------------------------------------|
| `BIND_ADDR`                   | `127.0.0.1:8000` | Server bind address and port                |
| `RUST_LOG`                    | `trace`          | Log level (trace, debug, info, warn, error) |
| `WORKERS`                     | `0`              | Number of worker threads (0 = auto-detect)  |
| `BUFFER_CLIENT_RECEIVE_SIZE`  | `32`             | Client receive buffer size                  |
| `BUFFER_READ_CLIENT_TIMEOUT`  | `3600s`          | Client read timeout                         |
| `BUFFER_WRITE_CLIENT_TIMEOUT` | `3600s`          | Client write timeout                        |
| `REQUEST_HEADER_LIMIT_BYTES`  | `4096`           | Maximum request header size                 |
| `COLORED_OUTPUT`              | `true`           | Enable colored log output                   |
| `COLORED_OUTPUT_FORCED`       | `false`          | Force colored output even in non-TTY        |

### Example Configuration

```bash
# Start server on all interfaces with info logging
BIND_ADDR=0.0.0.0:8080 RUST_LOG=info WORKERS=4 cargo run
```

## Testing

The project includes comprehensive tests for various components:

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test http_server_simple_valid_request

# Run tests with nextest (if installed)
cargo nextest run
```

### Test Categories

- **Unit Tests**: Individual component testing
- **Integration Tests**: End-to-end HTTP request/response testing
- **Configuration Tests**: Environment variable parsing and validation

## Development

### Development Features

The project uses Cargo features for optional functionality:

- `color` - Terminal color output support
- `humantime` - Human-readable time formatting
- `log-*` - Various logging levels (error, warn, info, verb, debug, trace)
- `profiling` - Performance profiling with flamegraph generation

### Building with Specific Features

```bash
# Build without color support
cargo build --no-default-features

# Build with only error logging
cargo build --no-default-features --features log-error

# Build with profiling enabled
cargo build --features profiling

# Build with all features
cargo build --all-features
```

## API Examples

### Basic HTTP Request

```bash
# Simple GET request
curl http://127.0.0.1:8000/

# Request with custom headers
curl -H "User-Agent: MyClient/1.0" http://127.0.0.1:8000/

# POST request with data
curl -X POST -d "Hello, Server!" http://127.0.0.1:8000/
```

### Using Bruno API Client

The project includes Bruno API collection files in the `rust-simple-httpd/` directory for testing:

```bash
# Install Bruno (if not already installed)
npm install -g @usebruno/cli

cd bruno

# Run the included API tests
bru run
```
