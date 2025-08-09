use std::net::Ipv4Addr;

use anyhow::Context;

pub struct NetworkConfig {
    pub client_ip: Ipv4Addr,
    pub server_ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub mtu: u16,
}

impl NetworkConfig {
    pub const BYTE_SIZE: usize = 3 * 4 + 2;
}

impl From<NetworkConfig> for [u8; NetworkConfig::BYTE_SIZE] {
    fn from(value: NetworkConfig) -> Self {
        let mut bytes = [0u8; NetworkConfig::BYTE_SIZE];
        bytes[0..4].copy_from_slice(&value.client_ip.octets());
        bytes[4..8].copy_from_slice(&value.server_ip.octets());
        bytes[8..12].copy_from_slice(&value.netmask.octets());
        bytes[12..14].copy_from_slice(&value.mtu.to_le_bytes());
        bytes
    }
}

impl From<&[u8; NetworkConfig::BYTE_SIZE]> for NetworkConfig {
    fn from(bytes: &[u8; NetworkConfig::BYTE_SIZE]) -> Self {
        let client_ip = Ipv4Addr::from_octets(bytes[0..4].try_into().unwrap());
        let server_ip = Ipv4Addr::from_octets(bytes[4..8].try_into().unwrap());
        let netmask = Ipv4Addr::from_octets(bytes[8..12].try_into().unwrap());
        let mtu = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
        Self {
            client_ip,
            server_ip,
            netmask,
            mtu,
        }
    }
}

impl TryFrom<&[u8]> for NetworkConfig {
    type Error = anyhow::Error;
    fn try_from(value: &[u8]) -> anyhow::Result<Self> {
        let bytes: &[u8; NetworkConfig::BYTE_SIZE] = value
            .try_into()
            .context("invalid NetworkConfig byte size")?;
        Ok(bytes.into())
    }
}
