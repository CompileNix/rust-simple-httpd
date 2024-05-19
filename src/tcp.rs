use std::net::{TcpListener, TcpStream};
use std::thread;
use std::sync::{Arc, mpsc, Mutex};
use crate::config::Config;
use crate::http::Server;
use crate::{Level, trace, verb, info};
use crate::worker::ThreadPool;

pub enum ConnectionHandlerMessage {
    NewConnection(TcpStream),
    Shutdown,
}

#[derive(Debug)]
pub struct ConnectionHandler {
    pub thread: thread::JoinHandle<()>,
    config: Config,
}

impl ConnectionHandler {
    /// Creates new `ConnectionHandler` instance.
    ///
    /// # Panics
    /// When `thread_count` is less then 1
    #[must_use]
    pub fn new(
        receiver: Arc<Mutex<mpsc::Receiver<ConnectionHandlerMessage>>>,
        config: Config,
    ) -> ConnectionHandler {
        let cfg = config.clone();
        let thread_pool = ThreadPool::new(config.clone());

        let connection_handler_config = config.clone();
        trace!(&cfg, "Start ConnectionHandler thread");
        let thread = thread::Builder::new()
            .name("ConnectionHandler".into())
            .spawn(move || {
                loop {
                    let cfg = connection_handler_config.clone();
                    let message = receiver.lock().unwrap().recv().unwrap();

                    match message {
                        ConnectionHandlerMessage::NewConnection(mut stream) => {
                            verb!(&cfg, "ConnectionHandler: received a new connection from HTTP Server");
                            thread_pool.execute(move |worker_id| {
                                trace!(&cfg, "[{worker_id}]: received a new job from the thread pool of the connection handler");
                                Server::handle_connection(&mut stream, worker_id, &cfg);
                            }).unwrap();
                        }
                        ConnectionHandlerMessage::Shutdown => {
                            verb!(&cfg, "ConnectionHandler: received shutdown signal");
                            break;
                        }
                    }
                }
                trace!(&cfg, "ConnectionHandler thread shutdown");
            }).unwrap();

        ConnectionHandler { thread, config }
    }

    pub fn bind(&self) -> TcpListener {
        let cfg = &self.config;
        let listener = TcpListener::bind(self.config.bind_addr.clone()).expect(&format!("Can't bind to {}", cfg.bind_addr));
        info!(cfg, "listening on {}", cfg.bind_addr);
        listener
    }
}
