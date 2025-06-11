use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use futures::io;
use tokio::{net::TcpStream, sync::watch};
use tokio_rustls::{rustls, TlsConnector};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tun::AbstractDevice;

use crate::{
    common::get_root_cert_store,
    config::{ClientConfig, TlsConfig},
    packet_stream::{PacketReceiver, PacketSender, TunReceiver, TunSender},
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
        let client_reader = client_reader.compat();
        let client_writer = client_writer.compat_write();
        let mut protocol_connection = Connection::new(client_reader, client_writer);

        let network_config = protocol_connection
            .receive_config()
            .await
            .context("could not receive network config")?;
        let tun_config = configure_tun(network_config);
        let device = tun::create_as_async(&tun_config)?;
        let mtu = device.mtu().unwrap() as usize;

        let (tun_writer, tun_reader) = device.split()?;
        let tun_receiver = TunReceiver::new(tun_reader, mtu);
        let tun_sender: TunSender = tun_writer.into();
        let (packet_sender, packet_receiver) = protocol_connection.into_parts();

        let send_fut = forward_packets(packet_receiver, tun_sender, self.stop_receiver.clone());
        let receive_fut = forward_packets(tun_receiver, packet_sender, self.stop_receiver.clone());
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

async fn forward_packets<R: PacketReceiver, S: PacketSender>(
    mut receiver: R,
    mut sender: S,
    mut stop_token: watch::Receiver<bool>,
) -> io::Result<()> {
    while !*stop_token.borrow_and_update() {
        let stop_fut = stop_token.changed();
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
                sender.send(&packet).await?;
            }
        }
    }
    sender.close().await
}
