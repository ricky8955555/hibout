use std::net::SocketAddr;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tracing::debug;

const MESSAGE_LENGTH: usize = 16;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Clone, Hash)]
pub struct Message {
    pub timestamp: u128,
}

impl Message {
    pub fn encode(&self) -> Vec<u8> {
        let mut encoded: Vec<u8> = vec![];
        encoded.extend_from_slice(&self.timestamp.to_be_bytes());
        encoded
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() != MESSAGE_LENGTH {
            bail!("invalid data");
        }
        let timestamp = u128::from_be_bytes(buf[0..16].try_into()?);
        Ok(Self { timestamp })
    }
}

pub struct Socket {
    socket: UdpSocket,
    peer: SocketAddr,
}

impl Socket {
    pub async fn connect(peer: SocketAddr, bind: SocketAddr, iface: Option<&str>) -> Result<Self> {
        let socket = UdpSocket::bind(bind).await?;
        socket.bind_device(iface.map(|s| s.as_bytes()))?;
        socket.connect(peer).await?;
        Ok(Self { socket, peer })
    }

    pub async fn send(&self, message: &Message) -> Result<()> {
        let buf = message.encode();
        self.socket.send(&buf[..]).await?;
        debug!("{} <- {:?}", self.peer.to_string(), message);
        Ok(())
    }

    pub async fn receive(&self) -> Result<Message> {
        let mut buf = [0; MESSAGE_LENGTH];
        let _ = self.socket.recv(&mut buf).await?;
        let message = Message::decode(&buf[..])?;
        debug!("{} -> {:?}", self.peer.to_string(), message);
        Ok(message)
    }

    pub fn try_receive(&self) -> Result<Message> {
        let mut buf = [0; MESSAGE_LENGTH];
        let _ = self.socket.try_recv(&mut buf)?;
        let message = Message::decode(&buf[..])?;
        debug!("{} -> {:?}", self.peer.to_string(), message);
        Ok(message)
    }
}
