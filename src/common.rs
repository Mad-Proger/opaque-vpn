/*
* Opaque VPN - VPN with main focus on opacity to DPI technologies
* Copyright (C) 2025 Mikhail Zaitsev
*
* This program is free software: you can redistribute it and/or modify
* it under the terms of the GNU General Public License as published by
* the Free Software Foundation, either version 3 of the License, or
* (at your option) any later version.
*/

use tokio_rustls::rustls::{pki_types::CertificateDer, RootCertStore};

pub fn get_root_cert_store(root_cert: CertificateDer<'static>) -> anyhow::Result<RootCertStore> {
    let mut store = RootCertStore::empty();
    store.add(root_cert)?;
    Ok(store)
}
