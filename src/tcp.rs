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
                    let loop_cfg = connection_handler_config.clone();
                    let message = receiver.lock().unwrap().recv().unwrap();

                    match message {
                        ConnectionHandlerMessage::NewConnection(mut stream) => {
                            let thread_pool_cfg = loop_cfg.clone();
                            let peer_addr = stream.peer_addr().unwrap();
                            trace!(&loop_cfg.clone(), "ConnectionHandler: received a connection from {}", peer_addr);
                            thread_pool.execute(move |worker_id| {
                                trace!(&thread_pool_cfg, "[{worker_id}]: received a job from the thread pool of the connection handler for {}", peer_addr);
                                Server::handle_connection(&mut stream, worker_id, &thread_pool_cfg);
                            }).unwrap();
                            trace!(&loop_cfg, "ConnectionHandler: delegated connection for {} to thread_pool", peer_addr);
                        }
                        ConnectionHandlerMessage::Shutdown => {
                            verb!(&loop_cfg, "ConnectionHandler: received shutdown signal");
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
