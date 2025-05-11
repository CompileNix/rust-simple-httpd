use std::sync::{Arc, mpsc, Mutex};
use std::sync::mpsc::SendError;
use std::thread;
use crate::config::Config;
use crate::{debug, info, trace, util, Level};

pub enum Message {
    NewJob(Job),
    Shutdown,
}

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Message>,
    config: Config,
}

pub trait FnBox {
    fn call_box(self: Box<Self>, worker_id: usize);
}

impl<F: FnOnce(usize)> FnBox for F {
    fn call_box(self: Box<F>, worker_id: usize) { (*self)(worker_id); }
}

pub type Job = Box<dyn FnBox + Send + 'static>;

impl ThreadPool {
    /// Create a new `ThreadPool`.
    ///
    /// The size is the number of threads in the pool.
    ///
    /// # Panics
    ///
    /// The `new` function will panic if the size is zero.
    #[must_use]
    pub fn new(config: Config) -> ThreadPool {
        let cfg = &config;

        let worker_count = util::available_parallelism_capped_at(config.workers);
        assert!(worker_count > 0);
        info!(cfg, "starting with {worker_count} threads");

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(worker_count);

        for id in 0..worker_count {
            workers.push(Worker::new(id, Arc::clone(&receiver), cfg));
        }

        ThreadPool { workers, sender, config }
    }

    /// Execute a function on the `ThreadPool`.
    ///
    /// # Errors
    /// A return value of `Err` means that the data will never be received, but a return value of
    /// `Ok` does not mean that the data will be received. It is possible for the corresponding
    /// receiver to hang up immediately after this function returns `Ok`.
    pub fn execute<F>(&self, f: F) -> Result<(), SendError<Message>>
        where F: FnOnce(usize) + Send + 'static, {

        self.sender.send(Message::NewJob(Box::new(f)))
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        let cfg = &self.config;
        debug!(cfg, "Thread pool: received shutdown signal");

        trace!(cfg, "Thread pool: sending shutdown message to all {} workers", self.workers.len());
        for _ in &mut self.workers {
            self.sender.send(Message::Shutdown).unwrap();
        }

        debug!(cfg, "Thread pool: waiting for all workers to stop");
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
        debug!(cfg, "Thread pool: all workers completed");
    }
}

pub struct Worker {
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    #[must_use] fn new(
        id: usize,
        receiver: Arc<Mutex<mpsc::Receiver<Message>>>,
        config: &Config,
    ) -> Worker {
        debug!(&config, "Worker thread {id}: startup");

        let thread_cfg = config.clone();
        let thread = thread::Builder::new()
            .name(format!("Worker {id}"))
            .spawn(move || loop {
            let message = receiver.lock().unwrap().recv().unwrap();

            match message {
                Message::NewJob(job) => {
                    trace!(&thread_cfg, "Worker thread {id}: received a job; executing");
                    job.call_box(id);
                    trace!(&thread_cfg, "Worker thread {id}: job complete");
                }
                Message::Shutdown => {
                    debug!(&thread_cfg, "Worker thread {id}: received shutdown signal");
                    break;
                }
            }
        }).unwrap();

        Worker {
            thread: Some(thread),
        }
    }
}
