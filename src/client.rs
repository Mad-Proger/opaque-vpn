use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::watch,
};
use tokio_rustls::{rustls, TlsConnector};
use tun::{AbstractDevice, AsyncDevice};

use crate::{
    common::{full_send, get_root_cert_store},
    config::{ClientConfig, TlsConfig},
    packet_stream::{TaggedPacketReceiver, TaggedPacketSender},
    protocol::{Connection, NetworkConfig},
};

pub struct Client {
    connector: TlsConnector,
    socket_address: SocketAddr,
    stop_sender: watch::Sender<bool>,
    stop_receiver: watch::Receiver<bool>,
}

impl Client {
    pub fn try_new(config: ClientConfig, tls: TlsConfig) -> anyhow::Result<Self> {
        let (sender, receiver) = watch::channel(false);
        Ok(Self {
            connector: Arc::new(configure_tls(tls)?).into(),
            socket_address: config.address,
            stop_sender: sender,
            stop_receiver: receiver,
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
        let mut protocol_connection = Connection::new(client_reader, client_writer);

        let network_config = protocol_connection
            .receive_config()
            .await
            .context("could not receive network config")?;
        let tun_config = configure_tun(network_config);
        let device = tun::create_as_async(&tun_config)?;
        let mtu = device.mtu().unwrap() as usize;

        let (tun_reader, tun_writer) = tokio::io::split(device);
        let (packet_sender, packet_receiver) = protocol_connection.into_parts();

        let send_fut = send_tun(packet_receiver, tun_writer, self.stop_receiver.clone());
        let receive_fut = receive_tun(packet_sender, tun_reader, mtu, self.stop_receiver.clone());
        tokio::try_join!(send_fut, receive_fut)?;

        Ok(())
    }
}

fn configure_tls(tls: TlsConfig) -> anyhow::Result<rustls::ClientConfig> {
    Ok(rustls::ClientConfig::builder()
        .with_root_certificates(get_root_cert_store(tls.root_certificate.clone())?)
        .with_client_auth_cert(vec![tls.certificate, tls.root_certificate], tls.key)?)
}

fn configure_tun(network_config: NetworkConfig) -> tun::Configuration {
    let mut config = tun::configure();
    config
        .address(network_config.client_ip)
        .destination(network_config.server_ip)
        .netmask(network_config.netmask)
        .mtu(network_config.mtu)
        .up();
    config
}

async fn send_tun<IO: AsyncRead + Unpin>(
    mut receiver: TaggedPacketReceiver<IO>,
    mut tun: WriteHalf<AsyncDevice>,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
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
    Ok(())
}

async fn receive_tun<IO: AsyncWrite + Unpin>(
    mut sender: TaggedPacketSender<IO>,
    mut tun: ReadHalf<AsyncDevice>,
    mtu: usize,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
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
                    return Ok(());
                }
                sender.send(&buf[..received]).await?;
            }
        }
    }
    sender.into_inner().shutdown().await?;
    Ok(())
}
