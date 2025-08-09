use std::{
    fs::File,
    io::Read,
    net::{Ipv4Addr, SocketAddr, ToSocketAddrs},
    path::Path,
};

use crate::AppWindow;
use anyhow::{Context, bail, ensure};
use serde::Deserialize;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};

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

impl Config {
    pub fn print_all(&self) {
        match &self.mode {
            Mode::Client(client_cfg) => {
                println!("Mode: Client");
                println!("  address: {}", client_cfg.address);
            }
            Mode::Server(server_cfg) => {
                println!("Mode: Server");
                println!("  port: {}", server_cfg.port);
                println!("  virtual_address: {}", server_cfg.virtual_address);
                println!("  subnet_mask: {}", server_cfg.subnet_mask);
            }
        }

        println!("TLS Configuration:");
        println!("  root_certificate: {:?}", self.tls.root_certificate);
        println!("  certificate:      {:?}", self.tls.certificate);
        println!("  key:              {:?}", self.tls.key);
    }
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

pub fn config_from_app(app: &AppWindow) -> anyhow::Result<Config> {
    let addr_str: String = app.get_address().into();
    let port_i32: i32 = app.get_port().into();
    let port: u16 = port_i32.try_into().context("port must be in 0..=65535")?;

    let socket_addr = (addr_str.as_str(), port)
        .to_socket_addrs()?
        .next()
        .context("could not resolve address")?;

    let client_cfg = ClientConfig {
        address: socket_addr,
    };

    let root_pem: String = app.get_root_cert().into();
    let cert_pem: String = app.get_cert().into();
    let key_pem: String = app.get_key().into();

    println!("{}", root_pem.as_str());

    let root_certificate = CertificateDer::from_pem_slice(root_pem.as_bytes())?;
    let certificate =
        CertificateDer::from_pem_slice(cert_pem.as_bytes()).context("invalid certificate PEM")?;
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes())?;

    let tls_cfg = TlsConfig {
        root_certificate,
        certificate,
        key,
    };

    Ok(Config {
        mode: Mode::Client(client_cfg),
        tls: tls_cfg,
    })
}
