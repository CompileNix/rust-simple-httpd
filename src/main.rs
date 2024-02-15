// vim: sw=4 et filetype=rust

// extern crate env_logger;
// extern crate log;
extern crate core;
extern crate num_cpus;

// use log.{debug, info, trace, warn};
use crate::config::Config;
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
mod log;
#[cfg(test)]
mod tests;
mod util;
mod http_server;

#[tokio::main]
async fn main() {
    // env_logger::Builder::from_env(Env::default().default_filter_or("info"))
    //     .format_timestamp_millis()
    //     .format_target(false)
    //     .format_indent(Some("[0000-00-00T00:00:00.000Z INFO ] ".len()))
    //     .init();

    let config = Config::default_from_env();
    log::verb(&format!("We are using the following config: {config}"));
    let log = log::Logger {
        level: config.log_level,
    };

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
                log.info(&format!("Received process signal {signal_text}"));
                process::exit(0);
            }
        });

    let server = http_server::HttpServer { config, log };
    let bind_addr = "127.0.0.1:8000";
    #[allow(clippy::expect_fun_call)]
    let listener = TcpListener::bind(bind_addr)
        .await
        .expect(&format!("Can't bind to {bind_addr}"));
    log.info(&format!("listening on {bind_addr}"));

    let worker_count = num_cpus::get();
    let semaphore = Arc::new(Semaphore::new(worker_count));

    loop {
        let (mut stream, socket_addr) = match listener.accept().await {
            Ok(accept) => accept,
            Err(error) => {
                log.warn(&format!(
                    "Couldn't receive new connection from socket listener: {error:?}"
                ));
                continue;
            }
        };

        log.debug(&format!(
            "Alright, we got a new connection from {socket_addr}."
        ));
        log.trace("Lets see if we have to wait for an available worker");
        log.trace(&format!(
            "{0} of {worker_count} workers are available",
            semaphore.available_permits()
        ));
        log.trace(
            "We are going ahead with acquiring a permit, or waiting for one to become available",
        );

        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .expect("Failed to receive permit from the semaphore");

        tokio::spawn(async move {
            server.clone().handle_connection(&mut stream).await;
            log.debug(&format!("We are done with a request from {socket_addr}"));
            drop(permit);
        });
    }
}
