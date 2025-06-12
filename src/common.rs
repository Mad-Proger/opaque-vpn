use tokio_rustls::rustls::{pki_types::CertificateDer, RootCertStore};

pub fn get_root_cert_store(root_cert: CertificateDer<'static>) -> anyhow::Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    store.add(root_cert)?;
    Ok(store)
}
