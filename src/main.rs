#![feature(rustc_attrs)]
#![allow(internal_features)]

// extern crate env_logger;
// extern crate log;

pub mod log;

use crate::config::Config;
use crate::log::Level;
use crate::log::{debug, info, trace, verb, warn};

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
#[cfg(test)]
mod tests;
mod util;

#[allow(unused_variables, unused_assignments, clippy::unwrap_used)]
#[tokio::main]
async fn main() {
    let mut cfg = Config::default_from_env();
    if !util::is_colored_output_avail(&cfg) {
        cfg.colored_output = false;
    }
    let cfg = &cfg;

    verb!(cfg, "We are using the following config: {cfg}");

    let signal_handler_config = cfg.clone();
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
                info!(
                    &signal_handler_config,
                    "Received process signal {signal_text}"
                );
                process::exit(0);
            }
        });

    let server = http_server::HttpServer {
        config: cfg.clone(),
    };
    let bind_addr = "127.0.0.1:8000";
    #[allow(clippy::expect_fun_call)]
    let listener = TcpListener::bind(bind_addr)
        .await
        .expect(&format!("Can't bind to {bind_addr}"));
    info!(cfg, "listening on {bind_addr}");

    let worker_count = util::available_parallelism_or(cfg.workers);
    let semaphore = Arc::new(Semaphore::new(worker_count));

    loop {
        let (mut stream, socket_addr) = match listener.accept().await {
            Ok(accept) => accept,
            Err(error) => {
                warn!(
                    cfg,
                    "Couldn't receive new connection from socket listener: {error:?}"
                );
                continue;
            }
        };

        debug!(cfg, "Alright, we got a new connection from {socket_addr}.");
        trace!(cfg, "Lets see if we have to wait for an available worker");
        trace!(
            cfg,
            "{0} of {worker_count} workers are available",
            semaphore.available_permits()
        );
        trace!(
            cfg,
            "We are going ahead with acquiring a permit, or waiting for one to become available"
        );

        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .expect("Failed to receive permit from the semaphore");

        let connection_server_context = server.clone();
        let tokio_worker_config = cfg.clone();
        tokio::spawn(async move {
            connection_server_context
                .handle_connection(&mut stream)
                .await;
            debug!(
                &tokio_worker_config,
                "We are done with a request from {socket_addr}"
            );
            drop(permit);
        });
    }
}
