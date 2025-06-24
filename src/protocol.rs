use std::net::Ipv4Addr;

use anyhow::Context;
use futures::io::{AsyncRead, AsyncWrite};

use crate::packet_stream::{
    PacketReceiver, PacketSender, TaggedPacketReceiver, TaggedPacketSender,
};

pub struct NetworkConfig {
    pub client_ip: Ipv4Addr,
    pub server_ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub mtu: u16,
}

const CONFIG_SIZE: usize = 3 * 4 + 2;

impl From<NetworkConfig> for [u8; CONFIG_SIZE] {
    fn from(value: NetworkConfig) -> Self {
        let mut bytes = [0u8; CONFIG_SIZE];
        bytes[0..4].copy_from_slice(&value.client_ip.octets());
        bytes[4..8].copy_from_slice(&value.server_ip.octets());
        bytes[8..12].copy_from_slice(&value.netmask.octets());
        bytes[12..14].copy_from_slice(&value.mtu.to_le_bytes());
        bytes
    }
}

impl From<&[u8; CONFIG_SIZE]> for NetworkConfig {
    fn from(bytes: &[u8; CONFIG_SIZE]) -> Self {
        let client_ip = Ipv4Addr::from_octets(bytes[0..4].try_into().unwrap());
        let server_ip = Ipv4Addr::from_octets(bytes[4..8].try_into().unwrap());
        let netmask = Ipv4Addr::from_octets(bytes[8..12].try_into().unwrap());
        let mtu = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
        Self {
            client_ip,
            server_ip,
            netmask,
            mtu,
        }
    }
}

impl TryFrom<&[u8]> for NetworkConfig {
    type Error = anyhow::Error;
    fn try_from(value: &[u8]) -> anyhow::Result<Self> {
        let bytes: &[u8; CONFIG_SIZE] = value
            .try_into()
            .context("invalid NetworkConfig byte size")?;
        Ok(bytes.into())
    }
}

pub struct Connection<Reader: Send, Writer: Send> {
    receiver: TaggedPacketReceiver<Reader>,
    sender: TaggedPacketSender<Writer>,
}

impl<Reader, Writer> Connection<Reader, Writer>
where
    Reader: AsyncRead + Unpin + Send,
    Writer: AsyncWrite + Unpin + Send,
{
    pub fn new(reader: Reader, writer: Writer) -> Self {
        Self {
            receiver: TaggedPacketReceiver::new(reader),
            sender: TaggedPacketSender::new(writer),
        }
    }

    pub async fn send_config(&mut self, config: NetworkConfig) -> std::io::Result<()> {
        let config_bytes: [u8; CONFIG_SIZE] = config.into();
        self.sender.send(&config_bytes).await
    }

    pub async fn receive_config(&mut self) -> anyhow::Result<NetworkConfig> {
        let config_bytes = self.receiver.receive().await?;
        config_bytes.as_ref().try_into()
    }

    pub fn into_parts(self) -> (TaggedPacketSender<Writer>, TaggedPacketReceiver<Reader>) {
        (self.sender, self.receiver)
    }
}
