use crate::{
    common::{full_send, get_root_cert_store, CONFIGURATION_SIZE},
    config::{ClientConfig, TlsConfig},
    packet_stream::{PacketReceiver, PacketSender},
    system_route::RouteManager,
    unsplit::Unsplit,
};
use anyhow::{bail, ensure, Context};
use log::error;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::watch,
};
use tokio_rustls::{rustls, TlsConnector};
use tun::{AbstractDevice, AsyncDevice};

pub struct Client {
    connector: TlsConnector,
    socket_address: SocketAddr,
    stop_sender: watch::Sender<bool>,
    stop_receiver: watch::Receiver<bool>,
    reroute: bool
}

impl Client {
    pub fn try_new(config: ClientConfig, tls: TlsConfig) -> anyhow::Result<Self> {
        let (sender, receiver) = watch::channel(false);
        Ok(Self {
            connector: Arc::new(configure_tls(tls)?).into(),
            socket_address: config.address,
            stop_sender: sender,
            stop_receiver: receiver,
            reroute: config.reroute,
        })
    }

    pub fn stop_sender(&self) -> watch::Sender<bool> {
        self.stop_sender.clone()
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
        let vpn_addr = device.address().unwrap();
        let (tun_reader, tun_writer) = tokio::io::split(device);

        let send_fut = send_tun(packet_receiver, tun_writer, self.stop_receiver.clone());
        let receive_fut = receive_tun(packet_sender, tun_reader, mtu, self.stop_receiver.clone());
        let (res_send, res_receive) = tokio::join!(send_fut, receive_fut);

        let mut route_manager = None;

        if self.reroute {
            let IpAddr::V4(remote_ipv4) = self.socket_address.ip() else {
                bail!("IPv6 rerouting is not supported");
            };
            let IpAddr::V4(vpn_ipv4) = vpn_addr else {
                bail!("IPv6 rerouting is not supported");
            };
            route_manager = RouteManager::try_new()?.into();
            route_manager.as_mut().unwrap().reroute(vpn_ipv4, remote_ipv4)?;
        }

        let mut unsplitter = Unsplit::new();
        if let Err(e) =
            res_send.and_then(|read_half| Ok(unsplitter.save_read_half(read_half.into_inner())?))
        {
            error!("{}", e);
        }
        if let Err(e) = res_receive
            .and_then(|write_half| Ok(unsplitter.save_write_half(write_half.into_inner())?))
        {
            error!("{}", e);
        }
        if let Some(mut stream) = unsplitter.unsplit() {
            stream.shutdown().await?;
        }

        if let Some(mut manager) = route_manager.take() {
            manager.reset()?;
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
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<PacketReceiver<IO>> {
    while !*stop_receiver.borrow_and_update() {
        let stop_fut = stop_receiver.changed();
        let packet_fut = receiver.receive();
        tokio::select! {
            res = stop_fut => {
                if res.is_err() {
                    break;
                }
                continue;
            }
            packet_res = packet_fut => {
                let packet = packet_res?;
                full_send(&mut tun, &packet).await?;
            }
        }
    }
    Ok(receiver)
}

async fn receive_tun<IO: AsyncWrite + Unpin>(
    mut sender: PacketSender<IO>,
    mut tun: ReadHalf<AsyncDevice>,
    mtu: usize,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<PacketSender<IO>> {
    let mut buf = vec![0u8; mtu];
    while !*stop_receiver.borrow_and_update() {
        let stop_fut = stop_receiver.changed();
        let read_fut = tun.read(buf.as_mut_slice());
        tokio::select! {
            res = stop_fut => {
                if res.is_err() {
                    break;
                }
                continue;
            }
            packet_res = read_fut => {
                let received = packet_res?;
                // maybe not?
                if received == 0 {
                    return Ok(sender);
                }
                sender.send(&buf[..received]).await?;
            }
        }
    }
    Ok(sender)
}
