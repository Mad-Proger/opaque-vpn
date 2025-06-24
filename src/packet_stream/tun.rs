use futures::io::{self, AsyncWriteExt};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use tun::{DeviceReader, DeviceWriter};

use crate::packet_stream::{PacketReceiver, PacketSender};

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
