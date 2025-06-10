use futures::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio_rustls::rustls::{pki_types::CertificateDer, RootCertStore};

pub fn get_root_cert_store(root_cert: CertificateDer<'static>) -> anyhow::Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    store.add(root_cert)?;
    Ok(store)
}

pub trait AsyncWriteFixed: AsyncWriteExt {
    async fn write_u16(&mut self, val: u16) -> io::Result<()>
    where
        Self: Unpin,
    {
        let bytes = val.to_le_bytes();
        self.write_all(&bytes).await
    }
}

impl<W: AsyncWriteExt> AsyncWriteFixed for W {}

pub trait AsyncReadFixed: AsyncReadExt {
    async fn read_u16(&mut self) -> io::Result<u16>
    where
        Self: Unpin,
    {
        let mut bytes = [0u8; size_of::<u16>()];
        self.read_exact(&mut bytes).await?;
        Ok(u16::from_le_bytes(bytes))
    }
}

impl<R: AsyncReadExt> AsyncReadFixed for R {}
