use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{
    packet_stream::{PacketReceiver, PacketSender, TaggedPacketReceiver, TaggedPacketSender},
    protocol::NetworkConfig,
};

pub struct Connection<Stream: Send>(Stream);

impl<Stream> Connection<Stream>
where
    Stream: AsyncRead + AsyncWrite + Unpin + Send,
{
    pub fn new(stream: Stream) -> Self {
        Self(stream)
    }

    pub async fn send_config(&mut self, config: NetworkConfig) -> std::io::Result<()> {
        let config_bytes: [u8; NetworkConfig::BYTE_SIZE] = config.into();
        self.0.write_all(&config_bytes).await
    }

    pub async fn receive_config(&mut self) -> std::io::Result<NetworkConfig> {
        let mut config_bytes = [0u8; NetworkConfig::BYTE_SIZE];
        self.0.read_exact(&mut config_bytes).await?;
        Ok((&config_bytes).into())
    }

    pub fn into_parts(self) -> (impl PacketSender, impl PacketReceiver) {
        let (reader, writer) = tokio::io::split(self.0);
        (
            TaggedPacketSender::new(writer.compat_write()),
            TaggedPacketReceiver::new(reader.compat()),
        )
    }
}
