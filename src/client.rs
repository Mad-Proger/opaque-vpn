use crate::{
    common::{full_send, get_root_cert_store, CONFIGURATION_SIZE},
    config::{ClientConfig, TlsConfig},
    packet_stream::{PacketReceiver, PacketSender},
};
use anyhow::{ensure, Context};
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadHalf, WriteHalf},
    net::TcpStream,
};
use tokio_rustls::{rustls, TlsConnector};
use tun::{AbstractDevice, AsyncDevice};

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
        let client = self
            .connector
            .connect(self.socket_address.ip().into(), socket)
            .await?;
        let (client_reader, client_writer) = tokio::io::split(client);
        let mut packet_receiver = PacketReceiver::new(client_reader);
        let packet_sender = PacketSender::new(client_writer);

        let tun_config = configure_tun(&mut packet_receiver).await?;
        let device = tun::create_as_async(&tun_config)?;
        let mtu = device.mtu().unwrap() as usize;
        let (tun_reader, tun_writer) = tokio::io::split(device);

        let send_fut = send_tun(packet_receiver, tun_writer);
        let receive_fut = receive_tun(packet_sender, tun_reader, mtu);
        let (res_send, res_receive) = tokio::join!(send_fut, receive_fut);
        if let Err(e) = res_send {
            eprintln!("send error: {}", e);
        }
        if let Err(e) = res_receive {
            eprintln!("receive error: {}", e);
        }

        Ok(())
    }
}

fn configure_tls(tls: TlsConfig) -> anyhow::Result<rustls::ClientConfig> {
    Ok(rustls::ClientConfig::builder()
        .with_root_certificates(get_root_cert_store(tls.root_certificate.clone())?)
        .with_client_auth_cert(vec![tls.certificate, tls.root_certificate], tls.key)?)
}

async fn configure_tun<IO: AsyncRead + Unpin>(
    receiver: &mut PacketReceiver<IO>,
) -> anyhow::Result<tun::Configuration> {
    let config = receiver
        .receive()
        .await
        .context("could not receive configuration")?;
    ensure!(
        config.len() == CONFIGURATION_SIZE,
        "invalid configuration format received"
    );

    let ip = Ipv4Addr::from_octets(config[..4].try_into().unwrap());
    let gateway = Ipv4Addr::from_octets(config[4..8].try_into().unwrap());
    let netmask = Ipv4Addr::from_octets(config[8..12].try_into().unwrap());
    let mtu = u16::from_le_bytes(config[12..].try_into().unwrap());

    let mut config = tun::configure();
    config
        .address(ip)
        .destination(gateway)
        .netmask(netmask)
        .mtu(mtu)
        .up();
    Ok(config)
}

async fn send_tun<IO: AsyncRead + Unpin>(
    mut receiver: PacketReceiver<IO>,
    mut tun: WriteHalf<AsyncDevice>,
) -> anyhow::Result<()> {
    loop {
        let packet = receiver.receive().await?;
        full_send(&mut tun, &packet).await?;
    }
}

async fn receive_tun<IO: AsyncWrite + Unpin>(
    mut sender: PacketSender<IO>,
    mut tun: ReadHalf<AsyncDevice>,
    mtu: usize,
) -> anyhow::Result<()> {
    let mut buf = vec![0u8; mtu];
    loop {
        let received = tun.read(buf.as_mut_slice()).await?;
        // maybe not?
        if received == 0 {
            return Ok(());
        }
        sender.send(&buf[..received]).await?;
    }
}
