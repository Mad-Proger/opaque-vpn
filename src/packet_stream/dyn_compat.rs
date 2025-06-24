use std::pin::Pin;

use futures::{future::Future, io};

use crate::packet_stream::PacketSender;

pub trait DynPacketSender: Send {
    fn send_dyn<'a>(
        &'a mut self,
        packet: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;

    fn close_dyn(&mut self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>>;
}

impl<S: PacketSender> DynPacketSender for S {
    fn send_dyn<'a>(
        &'a mut self,
        packet: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
        Box::pin(self.send(packet))
    }

    fn close_dyn(&mut self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(self.close())
    }
}
