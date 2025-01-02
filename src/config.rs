use anyhow::{anyhow, Context};
use std::{
    fs::File,
    io::Read,
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    str::FromStr,
};
use tokio_rustls::rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use yaml_rust::{yaml::Hash, Yaml, YamlLoader};

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

pub fn load_config<P: AsRef<Path>>(path: P) -> anyhow::Result<Config> {
    let mut file = File::open(path).context("could not open config file")?;
    let mut raw = String::new();
    _ = file
        .read_to_string(&mut raw)
        .context("could not read config file")?;

    let parsed = YamlLoader::load_from_str(&raw).context("could not parse config")?;
    read_config(parsed)
}

fn get_section<'a>(dict: &'a Hash, section: &str) -> anyhow::Result<&'a Yaml> {
    dict.get(&Yaml::String(section.into()))
        .context(format!("section '{}' not found", section))
}

fn get_string<'a>(dict: &'a Hash, key: &str) -> anyhow::Result<&'a String> {
    get_section(dict, key).and_then(|section| match section {
        Yaml::String(s) => Ok(s),
        _ => Err(anyhow!("'{}' is not a string", key)),
    })
}

fn read_config(parsed: Vec<Yaml>) -> anyhow::Result<Config> {
    let sections = parsed
        .into_iter()
        .next()
        .and_then(|node| {
            if let Yaml::Hash(hashmap) = node {
                Some(hashmap)
            } else {
                None
            }
        })
        .context("expected exactly 1 top-level entry")?;

    let mode = get_section(&sections, "general").and_then(read_mode)?;
    let tls = get_section(&sections, "tls").and_then(read_tls)?;
    Ok(Config { mode, tls })
}

fn read_mode(general: &Yaml) -> anyhow::Result<Mode> {
    let section = match general {
        Yaml::Hash(s) => Ok(s),
        _ => Err(anyhow!("section 'general' is not a dictionary")),
    }?;
    let mode_string = get_section(section, "mode").and_then(|mode_value| match mode_value {
        Yaml::String(mode_string) => Ok(mode_string),
        _ => Err(anyhow!("'mode' is not a string")),
    })?;

    match mode_string as &str {
        "client" => Ok(Mode::Client(read_client(section)?)),
        "server" => Ok(Mode::Server(read_server(section)?)),
        _ => Err(anyhow!("invalid 'mode' value")),
    }
}

fn read_tls(tls: &Yaml) -> anyhow::Result<TlsConfig> {
    let section = match tls {
        Yaml::Hash(s) => Ok(s),
        _ => Err(anyhow!("section 'tls' is not a dictionary")),
    }?;

    let root_cert_string = get_string(section, "root_certificate")?;
    let cert_string = get_string(section, "certificate")?;
    let key_string = get_string(section, "key")?;

    let root_cert = CertificateDer::from_pem_slice(root_cert_string.as_bytes())?;
    let cert = CertificateDer::from_pem_slice(cert_string.as_bytes())?;
    let key = PrivateKeyDer::from_pem_slice(key_string.as_bytes())?;

    Ok(TlsConfig {
        root_certificate: root_cert,
        certificate: cert,
        key,
    })
}

fn parse_ipv4(dict: &Hash, name: &str) -> anyhow::Result<Ipv4Addr> {
    get_section(dict, name).and_then(|address_value| match address_value {
        Yaml::String(address_string) => Ok(Ipv4Addr::from_str(address_string)?),
        _ => Err(anyhow!("invalid '{}' value", name)),
    })
}

fn read_server(general: &Hash) -> anyhow::Result<ServerConfig> {
    let port = get_section(general, "port").and_then(|port_value| match port_value {
        Yaml::Integer(port) => Ok(u16::try_from(*port)?),
        _ => Err(anyhow!("invalid 'port' value")),
    })?;
    let virtual_address = parse_ipv4(general, "virtual_address")?;
    let subnet_mask = parse_ipv4(general, "subnet_mask")?;
    Ok(ServerConfig {
        port,
        virtual_address,
        subnet_mask,
    })
}

fn read_client(general: &Hash) -> anyhow::Result<ClientConfig> {
    let address =
        get_section(general, "address").and_then(|address_value| match address_value {
            Yaml::String(address_string) => Ok(SocketAddr::parse_ascii(address_string.as_bytes())?),
            _ => Err(anyhow!("invalid 'address' value")),
        })?;
    Ok(ClientConfig { address })
}
