use futures::{future::Future, io};

pub trait PacketReceiver: Send {
    fn receive(&mut self) -> impl Future<Output = io::Result<Box<[u8]>>> + Send;
}

pub trait PacketSender: Send {
    fn send(&mut self, packet: &[u8]) -> impl Future<Output = io::Result<()>> + Send;

    fn close(&mut self) -> impl Future<Output = io::Result<()>> + Send;
}
