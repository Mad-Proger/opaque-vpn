use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
};

use anyhow::ensure;
use etherparse::IpSlice;
use log::{error, warn};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::{Mutex, RwLock},
};
use tokio_rustls::server::TlsStream;
use tun::AsyncDevice;

use crate::{ip_manager::IpManager, packet_stream::TaggedPacketSender};

type PacketSink = TaggedPacketSender<WriteHalf<TlsStream<TcpStream>>>;

pub struct Router {
    ip_manager: Mutex<IpManager>,
    routes: RwLock<HashMap<Ipv4Addr, Mutex<PacketSink>>>,
    tun_writer: Mutex<WriteHalf<AsyncDevice>>,
}

pub struct RouterConfig {
    pub address: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub mtu: u16,
}

pub struct IpLease {
    router: Arc<Router>,
    addr: Ipv4Addr,
}

enum RoutingResult {
    Ok,
    NotIP,
    NoIPv4,
    NoRoute,
    Error(anyhow::Error),
}

impl Router {
    pub fn new(config: RouterConfig, tun: AsyncDevice) -> Arc<Self> {
        let mut ip_manager = IpManager::new(config.address, config.netmask);
        ip_manager.block(config.address);

        let (tun_reader, tun_writer) = tokio::io::split(tun);
        let router = Arc::new(Self {
            ip_manager: ip_manager.into(),
            routes: HashMap::new().into(),
            tun_writer: tun_writer.into(),
        });

        tokio::spawn(router.clone().route_incoming(tun_reader, config.mtu));
        router
    }

    pub async fn route_packet(&self, packet: Box<[u8]>) -> anyhow::Result<()> {
        match self.route_local(&packet).await {
            RoutingResult::Error(err) => return Err(err),
            RoutingResult::Ok => return Ok(()),
            _ => {}
        };

        let mut lock = self.tun_writer.lock().await;
        let mut offset = 0;
        while offset < packet.len() {
            let sent = lock.write(&packet[offset..]).await?;
            ensure!(sent > 0, "could not write data to TUN interface");
            offset += sent;
        }

        Ok(())
    }

    pub async fn get_ip(self: Arc<Self>) -> Option<IpLease> {
        let mut lock = self.ip_manager.lock().await;
        lock.get_free().map(|ip| {
            lock.block(ip);
            IpLease {
                addr: ip,
                router: self.clone(),
            }
        })
    }

    async fn route_incoming(self: Arc<Self>, mut tun: ReadHalf<AsyncDevice>, mtu: u16) {
        let mut buf = vec![0u8; mtu as usize].into_boxed_slice();

        loop {
            let received = match tun.read(&mut buf).await {
                Ok(received) => received,
                Err(e) => {
                    error!("could not read packet from TUN: {e}");
                    break;
                }
            };

            match self.route_local(&buf[..received]).await {
                RoutingResult::Ok => {}
                RoutingResult::NotIP => warn!("destination IP does not belong to VPN"),
                RoutingResult::NoIPv4 => warn!("incoming packet without IPv4 destination"),
                RoutingResult::NoRoute => warn!("no route for incoming packet"),
                RoutingResult::Error(e) => error!("could not route incoming packet: {e}"),
            }
        }
    }

    async fn route_local(&self, packet: &[u8]) -> RoutingResult {
        let Ok(ip_slice) = IpSlice::from_slice(packet) else {
            return RoutingResult::NotIP;
        };
        let IpAddr::V4(destination) = ip_slice.destination_addr() else {
            return RoutingResult::NoIPv4;
        };
        let routes = self.routes.read().await;
        let Some(route) = routes.get(&destination) else {
            return RoutingResult::NoRoute;
        };
        if let Err(err) = route.lock().await.send(packet).await {
            return RoutingResult::Error(err.into());
        }
        RoutingResult::Ok
    }
}

impl IpLease {
    pub fn get_address(&self) -> Ipv4Addr {
        self.addr
    }

    pub async fn set_route(&self, route: PacketSink) {
        _ = self
            .router
            .routes
            .write()
            .await
            .insert(self.addr, route.into());
    }
}

impl Drop for IpLease {
    fn drop(&mut self) {
        let addr = self.addr;
        let router = self.router.clone();
        tokio::spawn(async move {
            router.routes.write().await.remove(&addr);
            router.ip_manager.lock().await.release(addr);
        });
    }
}
