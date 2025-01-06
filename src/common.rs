use futures::{Sink, SinkExt, Stream, StreamExt};
use tokio::{
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    select,
};
use tokio_rustls::{
    rustls::{pki_types::CertificateDer, RootCertStore},
    TlsStream,
};

pub const CONFIGURATION_SIZE: usize = 14;

pub fn get_root_cert_store(root_cert: CertificateDer<'static>) -> anyhow::Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    store.add(root_cert)?;
    Ok(store)
}

pub async fn full_send<S: AsyncWrite + Unpin>(sink: &mut S, data: &[u8]) -> io::Result<()> {
    let mut sent = 0;
    while sent < data.len() {
        sent += sink.write(&data[sent..]).await?;
    }
    sink.flush().await
}

pub async fn tls_link<IO, St, Sk>(
    tls: TlsStream<IO>,
    mut stream: St,
    mut sink: Sk,
    bufsize: usize,
) -> anyhow::Result<()>
where
    IO: AsyncRead + AsyncWrite + Unpin,
    St: Stream<Item = Box<[u8]>> + Unpin,
    Sk: Sink<Box<[u8]>, Error = anyhow::Error> + Unpin,
{
    let mut tls_wrapper = TlsWrapper::new(tls, bufsize);
    loop {
        let wait_tls = tls_wrapper.receive_packet();
        let wait_stream = stream.next();
        select! {
            packet = wait_tls => {
                sink.send(packet?).await?;
            }
            packet = wait_stream => {
                tls_wrapper.send_packet(packet.unwrap()).await?;
            }
        }
    }
}

struct TlsWrapper<IO> {
    stream: TlsStream<IO>,
    buffer: Box<[u8]>,
    buffered_size: usize,
}

impl<IO> TlsWrapper<IO>
where
    IO: AsyncRead + AsyncWrite + Unpin,
{
    fn new(stream: TlsStream<IO>, buffer_size: usize) -> Self {
        Self {
            stream,
            buffer: vec![0u8; buffer_size].into_boxed_slice(),
            buffered_size: 0,
        }
    }

    async fn receive_packet(&mut self) -> io::Result<Box<[u8]>> {
        if self.buffered_size == 0 {
            self.buffered_size = self.stream.read(&mut self.buffer).await?;
        }
        let mut res = vec![0u8; self.buffered_size].into_boxed_slice();
        res.copy_from_slice(&self.buffer[..self.buffered_size]);
        self.buffered_size = 0;
        Ok(res)
    }

    async fn send_packet(&mut self, packet: Box<[u8]>) -> io::Result<()> {
        full_send(&mut self.stream, &packet).await
    }
}
