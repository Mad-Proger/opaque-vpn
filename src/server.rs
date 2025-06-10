use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};

use anyhow::Context;
use futures::FutureExt;
use log::{error, info, warn};
use tokio::{
    io::AsyncRead,
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{
    rustls::{self, server::WebPkiClientVerifier},
    TlsAcceptor,
};
use tun::{AbstractDevice, AsyncDevice};

use crate::{
    common::get_root_cert_store,
    config::{ServerConfig, TlsConfig},
    packet_stream::TaggedPacketReceiver,
    protocol::{Connection, NetworkConfig},
    routing::{Router, RouterConfig},
};

pub struct Server {
    router: Arc<Router>,
    acceptor: TlsAcceptor,
    socket_address: SocketAddr,
    gateway: Ipv4Addr,
    netmask: Ipv4Addr,
    mtu: u16,
}

impl Server {
    pub fn try_new(config: ServerConfig, tls: TlsConfig) -> anyhow::Result<Arc<Self>> {
        let device = tun_create(&config)?;
        let mtu = device.mtu().context("could not get MTU")?;
        let router = Router::new(
            RouterConfig {
                address: config.virtual_address,
                netmask: config.subnet_mask,
                mtu,
            },
            device,
        );

        Ok(Self {
            router,
            acceptor: Arc::new(configure_tls(tls)?).into(),
            socket_address: SocketAddr::new(Ipv4Addr::from_bits(0).into(), config.port),
            gateway: config.virtual_address,
            netmask: config.subnet_mask,
            mtu,
        }
        .into())
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let listener = TcpListener::bind(self.socket_address).await?;
        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    info!("incoming connection from {addr}");
                    tokio::spawn(self.clone().handle_client(socket).map(|res| {
                        if let Err(e) = res {
                            warn!("{e}");
                        }
                    }));
                }
                Err(e) => error!("could not accept connection: {e}"),
            };
        }
    }

    async fn handle_client(self: Arc<Self>, socket: TcpStream) -> anyhow::Result<()> {
        let client = self.acceptor.accept(socket).await?;
        let (client_reader, client_writer) = tokio::io::split(client);
        let mut protocol_connection = Connection::new(client_reader, client_writer);

        let ip_lease = self
            .router
            .clone()
            .get_ip()
            .await
            .context("could not assign ip address")?;

        protocol_connection
            .send_config(NetworkConfig {
                client_ip: ip_lease.get_address(),
                server_ip: self.gateway,
                netmask: self.netmask,
                mtu: self.mtu,
            })
            .await
            .context("could not send network configuration")?;

        let (packet_sender, packet_receiver) = protocol_connection.into_parts();
        ip_lease.set_route(packet_sender).await;
        if let Err(e) = self.clone().forward_packets(packet_receiver).await {
            info!("connection terminated: {e}");
        }

        Ok(())
    }

    async fn forward_packets<IO: AsyncRead + Unpin>(
        self: Arc<Self>,
        mut packet_receiver: TaggedPacketReceiver<IO>,
    ) -> anyhow::Result<()> {
        loop {
            let packet = packet_receiver.receive().await?;
            self.router.route_packet(packet).await?;
        }
    }
}

fn tun_create(config: &ServerConfig) -> anyhow::Result<AsyncDevice> {
    let mut tun_config = tun::configure();
    tun_config
        .address(config.virtual_address)
        .netmask(config.subnet_mask)
        .up();
    let device = tun::create_as_async(&tun_config).context("could not create TUN interface")?;
    Ok(device)
}

fn configure_tls(tls: TlsConfig) -> anyhow::Result<rustls::ServerConfig> {
    Ok(rustls::ServerConfig::builder()
        .with_client_cert_verifier(
            WebPkiClientVerifier::builder(
                get_root_cert_store(tls.root_certificate.clone())?.into(),
            )
            .build()?,
        )
        .with_single_cert(vec![tls.certificate, tls.root_certificate], tls.key)?)
}
