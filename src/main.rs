#![feature(rustc_attrs)]
#![allow(internal_features)]

use std::process;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::log::{Level, debug, info, trace, verb, warn, error};
use signal_hook::consts::{SIGINT, SIGQUIT};
use signal_hook::iterator::Signals;

#[cfg(test)]
mod tests;
#[cfg(feature = "color")]
mod color;
mod config;
mod http;
mod log;
mod util;
mod worker;
mod tcp;

#[allow(unused_variables, unused_assignments, clippy::unwrap_used)]
fn main() {
    let mut cfg = Config::default_from_env();
    if !util::is_colored_output_avail(&cfg) {
        cfg.colored_output = false;
    }
    let cfg = &cfg;

    verb!(cfg, "final config: {cfg}");

    let (sender, receiver) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(receiver));

    trace!(cfg, "Start ProcessSignalHandler thread");
    let sender_signal = sender.clone();
    let signal_handler_config = cfg.clone();
    let _ = thread::Builder::new()
        .name("ProcessSignalHandler".into())
        .spawn(move || {
            let cfg = &signal_handler_config;
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

                info!(cfg, "ProcessSignalHandler: Received process signal {signal_text}");

                sender_signal.send(tcp::ConnectionHandlerMessage::Shutdown).unwrap_or_default();

                trace!(cfg, "ProcessSignalHandler: shutdown signal sent, waiting up to 10 seconds for graceful shutdown");
                thread::sleep(Duration::from_secs(10));
                error!(cfg, "ProcessSignalHandler: application did not shutdown within 10 seconds, force terminate");
                process::exit(1);
                // process::exit(0);
            }
        });

    let server = http::Server::new(cfg, receiver, sender);
    let _ = server.serve();
}
