use std::ffi::c_int;
use std::process;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "profiling")]
use std::fs::File;

use crate::config::Config;
use crate::log::Level;
#[cfg(feature = "log-error")]
use crate::log::error;
#[cfg(feature = "log-info")]
use crate::log::info;
#[cfg(feature = "log-warn")]
use crate::log::warn;
#[cfg(feature = "log-verb")]
use crate::log::verb;
#[cfg(feature = "log-debug")]
use crate::log::debug;
#[cfg(feature = "log-trace")]
use crate::log::trace;
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

fn main() {
    let mut cfg = Config::default_from_env().expect("Failed to get default config from env");

    #[cfg(feature = "profiling")]
    let guard = Some(pprof::ProfilerGuardBuilder::default().frequency(10000).build().unwrap());
    #[cfg(not(feature = "profiling"))]
    let guard = None::<()>; // Fake type when profiling is disabled

    if !util::is_colored_output_avail(&cfg) {
        cfg.colored_output = false;
    }

    verb!(&cfg, "final config: {cfg}");

    trace!(&cfg, "Start ProcessSignalHandler thread");
    let (sender, receiver) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(receiver));
    let sender_signal = sender.clone();
    let signal_handler_config = cfg.clone();
    let _ = thread::Builder::new()
        .name("ProcessSignalHandler".into())
        .spawn(move || process_signal_handler(&signal_handler_config, &sender_signal));

    let server = http::Server::new(&cfg, receiver, sender);
    let _ = server.serve();

    #[cfg(feature = "profiling")] {
        if let Some(ref guard) = guard
            && let Ok(report) = guard.report().build() {
            let file = File::create("flamegraph.svg").unwrap();
            let mut options = pprof::flamegraph::Options::default();
            options.image_width = Some(2500);
            report.flamegraph_with_options(file, &mut options).unwrap();
        }
    }
}

fn process_signal_handler(cfg: &Config, sender: &mpsc::Sender<tcp::ConnectionHandlerMessage>) {
    let mut signals = Signals::new([SIGINT, SIGQUIT])
        .expect("Error while initializing process signal trap");

    for signal in signals.forever() {
        match signal {
            SIGINT | SIGQUIT => shutdown_signal_handler(cfg, sender, signal),
            _ => { },
        }
    }
}

fn shutdown_signal_handler(cfg: &Config, sender_signal: &mpsc::Sender<tcp::ConnectionHandlerMessage>, signal: c_int) {
    let signal_text = match signal {
        SIGINT =>   "SIGINT".into(),
        SIGQUIT =>  "SIGQUIT".into(),
        _ =>        format!("{signal}"),
    };

    println!(); // add linebreak before log message because of ^C from stdin
    info!(cfg, "ProcessSignalHandler: Received process signal {signal_text}");

    sender_signal.send(tcp::ConnectionHandlerMessage::Shutdown).unwrap_or_default();

    trace!(cfg, "ProcessSignalHandler: shutdown signal sent, waiting up to 10 seconds for graceful shutdown");
    thread::sleep(Duration::from_secs(10));
    error!(cfg, "ProcessSignalHandler: application did not shutdown within 10 seconds, force terminate");
    process::exit(1);
}
