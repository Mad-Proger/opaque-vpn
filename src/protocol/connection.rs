use futures::io::{AsyncRead, AsyncWrite};

use crate::{
    packet_stream::{PacketReceiver, PacketSender, TaggedPacketReceiver, TaggedPacketSender},
    protocol::NetworkConfig,
};

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
        let config_bytes: [u8; NetworkConfig::BYTE_SIZE] = config.into();
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
