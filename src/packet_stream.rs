use crate::common::{AsyncReadFixed, AsyncWriteFixed};
use futures::{
    future::Future,
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use tun::{DeviceReader, DeviceWriter};

pub trait PacketReceiver: Send {
    fn receive(&mut self) -> impl Future<Output = io::Result<Box<[u8]>>> + Send;
}

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

pub struct TunReceiver {
    reader: DeviceReader,
    buffer: Vec<u8>,
}

impl TunReceiver {
    pub fn new(reader: DeviceReader, mtu: usize) -> Self {
        Self {
            reader,
            buffer: vec![0; mtu],
        }
    }
}

impl PacketReceiver for TunReceiver {
    async fn receive(&mut self) -> io::Result<Box<[u8]>> {
        // this is not cancel-safe, but we do not particularly care
        let cnt_read =
            <DeviceReader as tokio::io::AsyncReadExt>::read(&mut self.reader, &mut self.buffer)
                .await?;
        Ok(self.buffer[..cnt_read].into())
    }
}

pub trait PacketSender: Send {
    async fn send(&mut self, packet: &[u8]) -> io::Result<()>;

    async fn close(&mut self) -> io::Result<()>;
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

pub struct TunSender {
    wrapped: Compat<DeviceWriter>,
}

impl From<DeviceWriter> for TunSender {
    fn from(value: DeviceWriter) -> Self {
        Self {
            wrapped: value.compat_write(),
        }
    }
}

impl PacketSender for TunSender {
    async fn send(&mut self, packet: &[u8]) -> io::Result<()> {
        self.wrapped.write_all(packet).await?;
        self.wrapped.flush().await
    }

    async fn close(&mut self) -> io::Result<()> {
        self.wrapped.close().await
    }
}
