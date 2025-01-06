use crate::{
    common::{full_send, get_root_cert_store, tls_link, CONFIGURATION_SIZE},
    config::{ServerConfig, TlsConfig},
    ip_manager::IpManager,
};
use anyhow::Context;
use etherparse::IpSlice;
use futures::{sink, FutureExt};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    ops::DerefMut,
    sync::Arc,
};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{channel, Sender},
        Mutex, RwLock,
    },
};
use tokio_rustls::{
    rustls::{self, server::WebPkiClientVerifier},
    TlsAcceptor,
};
use tokio_stream::wrappers::ReceiverStream;
use tun::{AbstractDevice, AsyncDevice, DeviceReader, DeviceWriter};

pub struct Server {
    tun_reader: Mutex<DeviceReader>,
    tun_writer: Mutex<DeviceWriter>,
    ip_manager: Mutex<IpManager>,
    acceptor: TlsAcceptor,
    socket_address: SocketAddr,
    gateway: Ipv4Addr,
    netmask: Ipv4Addr,
    mtu: u16,
    routes: RwLock<HashMap<Ipv4Addr, Sender<Box<[u8]>>>>,
}

impl Server {
    pub fn try_new(config: ServerConfig, tls: TlsConfig) -> anyhow::Result<Arc<Self>> {
        let mut ip_manager = IpManager::new(config.virtual_address, config.subnet_mask);
        ip_manager.block(config.virtual_address);
        let device = tun_create(&config)?;
        let mtu = device.mtu().context("could not get MTU")?;
        let (writer, reader) = device.split()?;

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
        let mut client = self.acceptor.accept(socket).await?;
        let ip = self.get_ip().await?;

        let mut network_info = [0u8; CONFIGURATION_SIZE];
        network_info[..4].copy_from_slice(&ip.octets());
        network_info[4..8].copy_from_slice(&self.gateway.octets());
        network_info[8..12].copy_from_slice(&self.netmask.octets());
        network_info[12..].copy_from_slice(&self.mtu.to_le_bytes());
        full_send(&mut client, &network_info)
            .await
            .context("could not send network configuration")?;

        let (sender, receiver) = channel(1);
        self.routes.write().await.insert(ip, sender);
        let receiver_stream = ReceiverStream::new(receiver);
        // what the actual fuck?!
        let send_sink = Box::pin(sink::unfold(
            self.clone(),
            |server, packet: Box<[u8]>| async move {
                full_send(server.tun_writer.lock().await.deref_mut(), &packet).await?;
                Ok(server)
            },
        ));

        tokio::spawn(tls_link(
            client.into(),
            receiver_stream,
            send_sink,
            self.mtu as usize,
        ));
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
}

fn tun_create(config: &ServerConfig) -> anyhow::Result<AsyncDevice> {
    let mut tun_config = tun::configure();
    tun_config
        .address(config.virtual_address)
        .netmask(config.subnet_mask)
        .up();
    let device = tun::create(&tun_config)?;
    Ok(AsyncDevice::new(device)?)
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
    routes: &RwLock<HashMap<Ipv4Addr, Sender<Box<[u8]>>>>,
    packet: Box<[u8]>,
) -> anyhow::Result<()> {
    // TODO: implement broadcast
    let destination = get_packet_destination(&packet)?;
    let lock = routes.read().await;
    let route = lock
        .get(&destination)
        .context(format!("no route to {}", destination))?;
    route.send(packet).await?;
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
