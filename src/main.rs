// vim: sw=4 et filetype=rust
#![feature(rustc_attrs)]
#![allow(internal_features)]

// extern crate env_logger;
// extern crate log;
extern crate core;
extern crate num_cpus;

use crate::config::Config;
use crate::log::{debug, info, trace, verb, warn, Level};

use signal_hook::consts::{SIGINT, SIGQUIT};
use signal_hook::iterator::Signals;
use std::process;
use std::sync::Arc;
use std::thread;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;

#[cfg(feature = "color")]
mod color;
mod config;
mod http_server;
mod log;
#[cfg(test)]
mod tests;
mod util;

#[tokio::main]
async fn main() {
    let config = Config::default_from_env();
    let lvl = config.log_level;
    verb!(lvl, "We are using the following config: {config}");

    let _ = thread::Builder::new()
        .name("ProcessSignalHandler".into())
        .spawn(move || {
            let mut signals = Signals::new([SIGINT, SIGQUIT])
                .expect("Error while initializing process signal trap");
            if let Some(sig) = signals.forever().next() {
                let mut signal_text = format!("{sig}");
                if sig == SIGINT {
                    signal_text = "SIGINT".into();
                }
                if sig == SIGQUIT {
                    signal_text = "SIGQUIT".into();
                }
                info!(lvl, "Received process signal {signal_text}");
                process::exit(0);
            }
        });

    let server = http_server::HttpServer { config };
    let bind_addr = "127.0.0.1:8000";
    #[allow(clippy::expect_fun_call)]
    let listener = TcpListener::bind(bind_addr)
        .await
        .expect(&format!("Can't bind to {bind_addr}"));
    info!(lvl, "listening on {bind_addr}");

    let worker_count = num_cpus::get();
    let semaphore = Arc::new(Semaphore::new(worker_count));

    loop {
        let (mut stream, socket_addr) = match listener.accept().await {
            Ok(accept) => accept,
            Err(error) => {
                warn!(
                    lvl,
                    "Couldn't receive new connection from socket listener: {error:?}"
                );
                continue;
            }
        };

        debug!(lvl, "Alright, we got a new connection from {socket_addr}.");
        trace!(lvl, "Lets see if we have to wait for an available worker");
        trace!(
            lvl,
            "{0} of {worker_count} workers are available",
            semaphore.available_permits()
        );
        trace!(
            lvl,
            "We are going ahead with acquiring a permit, or waiting for one to become available"
        );

        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .expect("Failed to receive permit from the semaphore");

        tokio::spawn(async move {
            server.clone().handle_connection(&mut stream).await;
            debug!(lvl, "We are done with a request from {socket_addr}");
            drop(permit);
        });
    }
}
