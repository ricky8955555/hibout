use std::{
    cell::RefCell,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::Result;
use async_trait::async_trait;
use tokio::{
    net::UdpSocket,
    sync::{mpsc, Mutex},
    task, time,
};
use tracing::{debug, error, info, warn};

use crate::protocol::{Message, Socket};

pub struct Service {
    pub name: String,
    handlers: RefCell<Vec<task::JoinHandle<()>>>,
}

pub struct Context {
    pub latencies: Vec<u128>,
    pub cycle: usize,
}

impl Context {
    pub fn get_loss_count(&self) -> usize {
        self.cycle - self.latencies.len()
    }
}

#[async_trait]
pub trait Handler {
    async fn handle(&self, context: Context);
}

impl Service {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            handlers: RefCell::new(vec![]),
        }
    }

    pub async fn run(
        &self,
        bind: SocketAddr,
        iface: Option<&[u8]>,
        dest: SocketAddr,
        interval: Duration,
        delta: Duration,
        cycle: usize,
        handler: impl Handler + Send + Sync + 'static,
    ) -> Result<()> {
        let socket = UdpSocket::bind(bind).await?;
        socket.bind_device(iface)?;
        let socket = Arc::new(Mutex::new(Socket::new(socket)));
        let (tx, rx) = mpsc::channel(64);
        let (ptx, prx) = mpsc::channel(128);

        self.create_receiver(socket.clone(), tx, dest);
        self.create_sender(socket.clone(), interval, dest);
        self.create_income_handler(rx, ptx, cycle, interval + delta, dest);
        self.create_data_handler(prx, cycle, handler);

        Ok(())
    }

    fn create_data_handler(
        &self,
        mut rx: mpsc::Receiver<Vec<u128>>,
        cycle: usize,
        handler: impl Handler + Send + Sync + 'static,
    ) {
        let name = self.name.clone();

        let handler = tokio::spawn(async move {
            while let Some(latencies) = rx.recv().await {
                let context = Context { latencies, cycle };
                handler.handle(context).await;
                debug!("{} latencies was passed to post handler to handle", name);
            }
        });

        self.handlers.borrow_mut().push(handler);
    }

    fn create_income_handler(
        &self,
        mut rx: mpsc::Receiver<(Message, SocketAddr)>,
        tx: mpsc::Sender<Vec<u128>>,
        cycle: usize,
        timeout: Duration,
        dest: SocketAddr,
    ) {
        let name = self.name.clone();

        let handler = tokio::spawn(async move {
            let mut latencies = vec![];
            let mut counter = 0usize;

            loop {
                match time::timeout(timeout, rx.recv()).await {
                    Ok(Some((message, addr))) => {
                        let timestamp = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_millis();
                        let latency = match timestamp.checked_sub(message.timestamp) {
                            Some(num) => num,
                            None => {
                                error!("{} received a timestamp later than local", name);
                                continue;
                            }
                        };
                        info!("{} ({}) -> latency: {} ms", name, addr.to_string(), latency);
                        latencies.push(latency);
                    }
                    Ok(None) => break,
                    Err(_) => warn!(
                        "no package received from {} ({}) within given duration",
                        name,
                        dest.to_string()
                    ),
                }

                counter += 1;

                if counter == cycle {
                    let count = latencies.len();
                    debug!(
                        "{} counter reached cycle. received: {}, lost: {}, total: {}. latencies: {:?}.",
                        name,
                        count,
                        cycle - count,
                        cycle,
                        latencies,
                    );
                    if let Err(ref e) = tx.send(latencies.clone()).await {
                        error!("error occurred when data sending through mspc: {}", e)
                    }
                    debug!(
                        "{} latencies was sent to post handling task to handle",
                        name
                    );
                    latencies.clear();
                    counter = 0;
                }
            }
        });

        self.handlers.borrow_mut().push(handler);
    }

    fn create_receiver(
        &self,
        socket: Arc<Mutex<Socket>>,
        tx: mpsc::Sender<(Message, SocketAddr)>,
        dest: SocketAddr,
    ) {
        let handler = tokio::spawn(async move {
            loop {
                match socket.lock().await.receive().await {
                    Ok((message, addr)) => {
                        if addr != dest {
                            error!(
                                "received a message from unknown address {}, dropped",
                                addr.to_string()
                            );
                            continue;
                        }
                        if let Err(ref e) = tx.send((message, addr)).await {
                            error!("error occurred when data sending through mspc: {}", e)
                        }
                    }
                    Err(e) => {
                        error!("error occurred when receiving from socket: {}", e);
                        break;
                    }
                }
            }
        });

        self.handlers.borrow_mut().push(handler);
    }

    fn create_sender(&self, socket: Arc<Mutex<Socket>>, interval: Duration, addr: SocketAddr) {
        let handler = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            loop {
                interval.tick().await;
                let timestamp = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_millis();
                let message = Message { timestamp };
                if let Err(ref e) = socket.lock().await.send(&message, addr).await {
                    error!("error occurred when data sending through mspc: {}", e)
                }
            }
        });

        self.handlers.borrow_mut().push(handler);
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        for handler in self.handlers.borrow().iter() {
            handler.abort();
        }
    }
}
