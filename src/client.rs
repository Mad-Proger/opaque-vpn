use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use anyhow::ensure;
use futures::pin_mut;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{client::TlsStream, rustls, TlsConnector};
use tun::{AbstractDevice, AsyncDevice, DeviceReader, DeviceWriter};

use crate::{
    common::{full_send, get_root_cert_store, tls_link, CONFIGURATION_SIZE},
    config::{ClientConfig, TlsConfig},
};

pub struct Client {
    connector: TlsConnector,
    socket_address: SocketAddr,
}

impl Client {
    pub fn try_new(config: ClientConfig, tls: TlsConfig) -> anyhow::Result<Self> {
        Ok(Self {
            connector: Arc::new(configure_tls(tls)?).into(),
            socket_address: config.address,
        })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let socket = TcpStream::connect(self.socket_address).await?;
        let mut client = self
            .connector
            .connect(self.socket_address.ip().into(), socket)
            .await?;

        let tun_config = configure_tun(&mut client).await?;
        let device = tun::create_as_async(&tun_config)?;
        let mtu = device.mtu().unwrap() as usize;
        let (writer, reader) = device.split()?;
        let reader_stream =
            futures::stream::unfold(reader, move |reader| receive_tun_packet(reader, mtu));
        let writer_sink =
            futures::sink::unfold(writer, move |mut writer, packet: Box<[u8]>| async move {
                if let Err(e) = full_send(&mut writer, &packet).await {
                    eprintln!("could not send packet to TUN: {}", e);
                }
                Ok(writer)
            });

        pin_mut!(reader_stream);
        pin_mut!(writer_sink);
        tls_link(client.into(), reader_stream, writer_sink, mtu).await
    }
}

fn configure_tls(tls: TlsConfig) -> anyhow::Result<rustls::ClientConfig> {
    Ok(rustls::ClientConfig::builder()
        .with_root_certificates(get_root_cert_store(tls.root_certificate.clone())?)
        .with_client_auth_cert(vec![tls.certificate, tls.root_certificate], tls.key)?)
}

async fn configure_tun(client: &mut TlsStream<TcpStream>) -> anyhow::Result<tun::Configuration> {
    let mut buf = [0; CONFIGURATION_SIZE];
    let received = client.read(&mut buf).await?;
    ensure!(
        received == CONFIGURATION_SIZE,
        "invalid configuration format received"
    );

    let ip = Ipv4Addr::from_octets(buf[..4].try_into().unwrap());
    let gateway = Ipv4Addr::from_octets(buf[4..8].try_into().unwrap());
    let netmask = Ipv4Addr::from_octets(buf[8..12].try_into().unwrap());
    let mtu = u16::from_le_bytes(buf[12..].try_into().unwrap());

    let mut config = tun::configure();
    config
        .address(ip)
        .destination(gateway)
        .netmask(netmask)
        .mtu(mtu)
        .up();
    Ok(config)
}

async fn receive_tun_packet(
    mut reader: DeviceReader,
    mtu: usize,
) -> Option<(Box<[u8]>, DeviceReader)> {
    let mut buf = vec![0u8; mtu];
    let received = match reader.read(buf.as_mut_slice()).await {
        Ok(cnt) => cnt,
        Err(e) => {
            eprintln!("could not receive packet from TUN: {}", e);
            return None;
        }
    };
    if received == 0 {
        return None;
    }
    buf.resize(received, 0);
    Some((buf.into_boxed_slice(), reader))
}
