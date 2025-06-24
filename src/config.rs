use std::{
    fs::File,
    io::Read,
    net::{Ipv4Addr, SocketAddr, ToSocketAddrs},
    path::Path,
};

use anyhow::{bail, ensure, Context};
use serde::Deserialize;
use tokio_rustls::rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};

pub struct ClientConfig {
    pub address: SocketAddr,
}

pub struct ServerConfig {
    pub port: u16,
    pub virtual_address: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
}

pub enum Mode {
    Client(ClientConfig),
    Server(ServerConfig),
}

pub struct TlsConfig {
    pub root_certificate: CertificateDer<'static>,
    pub certificate: CertificateDer<'static>,
    pub key: PrivateKeyDer<'static>,
}

pub struct Config {
    pub mode: Mode,
    pub tls: TlsConfig,
}

#[derive(Deserialize)]
struct RawClient {
    address: String,
    port: u16,
}

#[derive(Deserialize)]
struct RawServer {
    port: u16,
    virtual_address: Ipv4Addr,
    subnet_mask: Ipv4Addr,
}

#[derive(Deserialize)]
struct RawTls {
    root_certificate: String,
    certificate: String,
    key: String,
}

#[derive(Deserialize)]
struct RawConfig {
    client: Option<RawClient>,
    server: Option<RawServer>,
    tls: RawTls,
}

pub fn load_config<P: AsRef<Path>>(path: P) -> anyhow::Result<Config> {
    let mut file = File::open(path).context("could not open config file")?;
    let mut raw = String::new();
    _ = file
        .read_to_string(&mut raw)
        .context("could not read config file")?;

    let raw_config: RawConfig = toml::from_str(&raw).context("could not parse config")?;
    read_config(raw_config)
}

fn read_config(raw_config: RawConfig) -> anyhow::Result<Config> {
    ensure!(
        raw_config.client.is_none() || raw_config.server.is_none(),
        "config cannot contain both 'client' and 'server' sections"
    );

    let mode = if let Some(raw_client) = raw_config.client {
        Mode::Client(read_client(raw_client)?)
    } else if let Some(raw_server) = raw_config.server {
        Mode::Server(read_server(raw_server)?)
    } else {
        bail!("config must contain either 'client' or 'server' section");
    };
    let tls = read_tls(raw_config.tls)?;

    Ok(Config { mode, tls })
}

fn read_client(raw_client: RawClient) -> anyhow::Result<ClientConfig> {
    let address = (raw_client.address.as_str(), raw_client.port)
        .to_socket_addrs()?
        .next()
        .context("could not parse server address")?;
    Ok(ClientConfig { address })
}

fn read_server(raw_server: RawServer) -> anyhow::Result<ServerConfig> {
    Ok(ServerConfig {
        port: raw_server.port,
        virtual_address: raw_server.virtual_address,
        subnet_mask: raw_server.subnet_mask,
    })
}

fn read_tls(raw_tls: RawTls) -> anyhow::Result<TlsConfig> {
    let root_cert = CertificateDer::from_pem_slice(raw_tls.root_certificate.as_bytes())?;
    let cert = CertificateDer::from_pem_slice(raw_tls.certificate.as_bytes())?;
    let key = PrivateKeyDer::from_pem_slice(raw_tls.key.as_bytes())?;

    Ok(TlsConfig {
        root_certificate: root_cert,
        certificate: cert,
        key,
    })
}
