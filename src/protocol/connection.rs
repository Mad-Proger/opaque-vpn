use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{
    packet_stream::{PacketReceiver, PacketSender, TaggedPacketReceiver, TaggedPacketSender},
    protocol::NetworkConfig,
};

pub struct Connection<Reader: Send, Writer: Send> {
    reader: Reader,
    writer: Writer,
}

impl<Reader, Writer> Connection<Reader, Writer>
where
    Reader: AsyncRead + Unpin + Send,
    Writer: AsyncWrite + Unpin + Send,
{
    pub fn new(reader: Reader, writer: Writer) -> Self {
        Self { reader, writer }
    }

    pub async fn send_config(&mut self, config: NetworkConfig) -> std::io::Result<()> {
        let config_bytes: [u8; NetworkConfig::BYTE_SIZE] = config.into();
        self.writer.write_all(&config_bytes).await
    }

    pub async fn receive_config(&mut self) -> anyhow::Result<NetworkConfig> {
        let mut config_bytes = [0u8; NetworkConfig::BYTE_SIZE];
        self.reader.read_exact(&mut config_bytes).await?;
        config_bytes.as_ref().try_into()
    }

    pub fn into_parts(self) -> (impl PacketSender, impl PacketReceiver) {
        (
            TaggedPacketSender::new(self.writer),
            TaggedPacketReceiver::new(self.reader),
        )
    }
}
