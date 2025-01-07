use crate::{
    common::{get_root_cert_store, CONFIGURATION_SIZE},
    config::{ServerConfig, TlsConfig},
    ip_manager::IpManager,
    packet_stream::{PacketReceiver, PacketSender},
};
use anyhow::Context;
use etherparse::IpSlice;
use futures::FutureExt;
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::{TcpListener, TcpStream},
    sync::Mutex,
};
use tokio_rustls::{
    rustls::{self, server::WebPkiClientVerifier},
    server::TlsStream,
    TlsAcceptor,
};
use tun::{AbstractDevice, AsyncDevice};

pub struct Server {
    tun_reader: Mutex<ReadHalf<AsyncDevice>>,
    tun_writer: Mutex<WriteHalf<AsyncDevice>>,
    ip_manager: Mutex<IpManager>,
    acceptor: TlsAcceptor,
    socket_address: SocketAddr,
    gateway: Ipv4Addr,
    netmask: Ipv4Addr,
    mtu: u16,
    routes: Mutex<HashMap<Ipv4Addr, PacketSender<WriteHalf<TlsStream<TcpStream>>>>>,
}

impl Server {
    pub fn try_new(config: ServerConfig, tls: TlsConfig) -> anyhow::Result<Arc<Self>> {
        let mut ip_manager = IpManager::new(config.virtual_address, config.subnet_mask);
        ip_manager.block(config.virtual_address);
        let device = tun_create(&config)?;
        let mtu = device.mtu().context("could not get MTU")?;
        // native split doesn't work
        let (reader, writer) = tokio::io::split(device);

        Ok(Self {
            tun_reader: reader.into(),
            tun_writer: writer.into(),
            ip_manager: ip_manager.into(),
            acceptor: Arc::new(configure_tls(tls)?).into(),
            socket_address: SocketAddr::new(Ipv4Addr::from_bits(0).into(), config.port),
            gateway: config.virtual_address,
            netmask: config.subnet_mask,
            mtu,
            routes: HashMap::new().into(),
        }
        .into())
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let listener = TcpListener::bind(self.socket_address).await?;
        tokio::spawn(self.clone().route_incoming());
        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    println!("incoming connection from {}", addr);
                    tokio::spawn(self.clone().handle_client(socket).map(|res| {
                        if let Err(e) = res {
                            eprintln!("{}", e);
                        }
                    }));
                }
                Err(e) => eprintln!("could not accept connection: {}", e),
            };
        }
    }

    async fn handle_client(self: Arc<Self>, socket: TcpStream) -> anyhow::Result<()> {
        let client = self.acceptor.accept(socket).await?;
        let (client_reader, client_writer) = tokio::io::split(client);
        let packet_receiver = PacketReceiver::new(client_reader);
        let mut packet_sender = PacketSender::new(client_writer);

        let ip = self.get_ip().await?;
        let mut network_info = [0u8; CONFIGURATION_SIZE];
        network_info[..4].copy_from_slice(&ip.octets());
        network_info[4..8].copy_from_slice(&self.gateway.octets());
        network_info[8..12].copy_from_slice(&self.netmask.octets());
        network_info[12..].copy_from_slice(&self.mtu.to_le_bytes());
        let res = packet_sender
            .send(&network_info)
            .await
            .context("could not send network configuration");
        if let Err(e) = res {
            self.ip_manager.lock().await.release(ip);
            return Err(e);
        }

        self.routes.lock().await.insert(ip, packet_sender);
        if let Err(e) = self.clone().forward_packets(packet_receiver).await {
            eprintln!("connection terminated: {}", e);
        }

        self.ip_manager.lock().await.release(ip);
        Ok(())
    }

    async fn route_incoming(self: Arc<Self>) {
        let mut buf = vec![0; self.mtu as usize];
        loop {
            let packet_size = match self.tun_reader.lock().await.read(buf.as_mut_slice()).await {
                Ok(size) => size,
                Err(e) => {
                    eprintln!("could not read packet from TUN: {}", e);
                    continue;
                }
            };
            if packet_size == 0 {
                break;
            }
            if let Err(e) = route_packet(&self.routes, buf[..packet_size].into()).await {
                eprintln!("could not route incoming packet: {}", e);
            }
        }
    }

    async fn get_ip(&self) -> anyhow::Result<Ipv4Addr> {
        let mut lock = self.ip_manager.lock().await;
        let ip = lock.get_free().context("no IP addresses available")?;
        lock.block(ip);
        Ok(ip)
    }

    async fn forward_packets<IO: AsyncRead + Unpin>(
        self: Arc<Self>,
        mut packet_receiver: PacketReceiver<IO>,
    ) -> anyhow::Result<()> {
        loop {
            let packet = packet_receiver.receive().await?;
            eprintln!("forwarding packet");
            _ = self.tun_writer.lock().await.write(&packet).await?;
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

async fn route_packet(
    routes: &Mutex<HashMap<Ipv4Addr, PacketSender<WriteHalf<TlsStream<TcpStream>>>>>,
    packet: Box<[u8]>,
) -> anyhow::Result<()> {
    // TODO: implement broadcast
    let destination = get_packet_destination(&packet)?;
    eprintln!("packet to {}", destination);
    let mut lock = routes.lock().await;
    let route = lock
        .get_mut(&destination)
        .context(format!("no route to {}", destination))?;
    route.send(&packet).await?;
    Ok(())
}

fn get_packet_destination(packet: &[u8]) -> anyhow::Result<Ipv4Addr> {
    // maybe don't need to parse whole packet, only header
    IpSlice::from_slice(packet)
        .context("could not parse IP packet")?
        .ipv4()
        .map(|packet| Ipv4Addr::from_octets(packet.header().destination()))
        .context("packet does not have IPv4 address")
}
