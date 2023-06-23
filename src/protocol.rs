use std::net::SocketAddr;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tracing::debug;

const MESSAGE_LENGTH: usize = 16;

fn purify_addr(addr: SocketAddr) -> SocketAddr {
    SocketAddr::new(addr.ip(), addr.port())
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Clone, Hash)]
pub struct Message {
    pub timestamp: u128,
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut encoded: Vec<u8> = vec![];
        encoded.extend_from_slice(&self.timestamp.to_ne_bytes());
        encoded
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() != MESSAGE_LENGTH {
            bail!("invalid data");
        }
        let timestamp = u128::from_ne_bytes(buf[0..16].try_into()?);
        Ok(Self { timestamp })
    }
}

pub struct Socket {
    socket: UdpSocket,
}

impl Socket {
    pub fn new(socket: UdpSocket) -> Self {
        Self { socket }
    }

    pub async fn send(&self, message: &Message, addr: SocketAddr) -> Result<()> {
        let buf = message.encode();
        self.socket.send_to(&buf[..], addr).await?;
        debug!("{} <- {:?}", addr.to_string(), message);
        Ok(())
    }

    pub async fn receive(&self) -> Result<(Message, SocketAddr)> {
        let mut buf = [0; MESSAGE_LENGTH];
        let (_, addr) = self.socket.recv_from(&mut buf).await?;
        let addr = purify_addr(addr);
        let message = Message::decode(&buf[..])?;
        debug!("{} -> {:?}", addr.to_string(), message);
        Ok((message, addr))
    }

    pub fn try_receive(&self) -> Result<(Message, SocketAddr)> {
        let mut buf = [0; MESSAGE_LENGTH];
        let (_, addr) = self.socket.try_recv_from(&mut buf)?;
        let addr = purify_addr(addr);
        let message = Message::decode(&buf[..])?;
        debug!("{} -> {:?}", addr.to_string(), message);
        Ok((message, addr))
    }
}
