use crate::config::{ClientConfig, TlsConfig};

pub struct Client {}

impl Client {
    pub fn try_new(config: ClientConfig, tls: TlsConfig) -> anyhow::Result<Self> {
        unimplemented!()
    }

    pub async fn run(self) -> anyhow::Result<()> {
        unimplemented!()
    }
}
