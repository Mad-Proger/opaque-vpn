use futures::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::packet_stream::{
    util::{AsyncReadFixed, AsyncWriteFixed},
    PacketReceiver, PacketSender,
};

pub struct TaggedPacketReceiver<IO: Send> {
    stream: IO,
}

impl<IO: AsyncRead + Unpin + Send> TaggedPacketReceiver<IO> {
    pub fn new(stream: IO) -> Self {
        Self { stream }
    }
}

impl<IO: AsyncRead + Unpin + Send> PacketReceiver for TaggedPacketReceiver<IO> {
    async fn receive(&mut self) -> io::Result<Box<[u8]>> {
        let packet_size = self.stream.read_u16().await? as usize;
        let mut packet = vec![0u8; packet_size].into_boxed_slice();

        let mut offset = 0;
        while offset < packet_size {
            let received = self.stream.read(&mut packet[offset..]).await?;
            if received == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            offset += received;
        }

        Ok(packet)
    }
}

pub struct TaggedPacketSender<IO> {
    stream: IO,
}

impl<IO: AsyncWrite + Unpin> TaggedPacketSender<IO> {
    pub fn new(stream: IO) -> Self {
        Self { stream }
    }
}

impl<IO: AsyncWrite + Unpin + Send> PacketSender for TaggedPacketSender<IO> {
    async fn send(&mut self, packet: &[u8]) -> io::Result<()> {
        let packet_size = match u16::try_from(packet.len()) {
            Ok(s) => s,
            Err(_) => return Err(io::ErrorKind::FileTooLarge.into()),
        };
        self.stream.write_u16(packet_size).await?;

        let mut offset = 0;
        while offset < packet.len() {
            let written = self.stream.write(&packet[offset..]).await?;
            if written == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            offset += written;
        }
        self.stream.flush().await
    }

    async fn close(&mut self) -> io::Result<()> {
        self.stream.close().await
    }
}
