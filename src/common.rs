use tokio::io::{self, AsyncWrite, AsyncWriteExt};
use tokio_rustls::rustls::{pki_types::CertificateDer, RootCertStore};

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
