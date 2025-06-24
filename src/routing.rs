use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
};

use etherparse::IpSlice;
use log::{error, warn};
use tokio::sync::{Mutex, RwLock};

use crate::{
    ip_manager::IpManager,
    packet_stream::{DynPacketSender, PacketReceiver, PacketSender},
};

type PacketSink = Box<dyn DynPacketSender>;

pub struct Router<S: PacketSender> {
    ip_manager: Mutex<IpManager>,
    routes: RwLock<HashMap<Ipv4Addr, Mutex<PacketSink>>>,
    tun_writer: Mutex<S>,
}

pub struct RouterConfig {
    pub address: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

pub struct IpLease<S: PacketSender + 'static> {
    router: Arc<Router<S>>,
    addr: Ipv4Addr,
}

enum RoutingResult {
    Ok,
    NotIP,
    NoIPv4,
    NoRoute,
    Error(anyhow::Error),
}

impl<S: PacketSender + 'static> Router<S> {
    pub fn new<R: PacketReceiver + 'static>(
        config: RouterConfig,
        tun_sender: S,
        tun_receiver: R,
    ) -> Arc<Self> {
        let mut ip_manager = IpManager::new(config.address, config.netmask);
        ip_manager.block(config.address);

        let router = Arc::new(Self {
            ip_manager: ip_manager.into(),
            routes: HashMap::new().into(),
            tun_writer: tun_sender.into(),
        });

        tokio::spawn(router.clone().route_incoming(tun_receiver));
        router
    }

    pub async fn route_packet(&self, packet: Box<[u8]>) -> anyhow::Result<()> {
        match self.route_local(&packet).await {
            RoutingResult::Error(err) => return Err(err),
            RoutingResult::Ok => return Ok(()),
            _ => {}
        };

        let mut lock = self.tun_writer.lock().await;
        lock.send(&packet).await?;
        Ok(())
    }

    pub async fn get_ip(self: Arc<Self>) -> Option<IpLease<S>> {
        let mut lock = self.ip_manager.lock().await;
        lock.get_free().map(|ip| {
            lock.block(ip);
            IpLease {
                addr: ip,
                router: self.clone(),
            }
        })
    }

    async fn route_incoming<R: PacketReceiver>(self: Arc<Self>, mut tun_receiver: R) {
        loop {
            let packet = match tun_receiver.receive().await {
                Ok(packet) => packet,
                Err(e) => {
                    error!("could not read packet from tun: {e}");
                    continue;
                }
            };

            match self.route_local(&packet).await {
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
        if let Err(err) = route.lock().await.send_dyn(packet).await {
            return RoutingResult::Error(err.into());
        }
        RoutingResult::Ok
    }
}

impl<S: PacketSender + 'static> IpLease<S> {
    pub fn get_address(&self) -> Ipv4Addr {
        self.addr
    }

    pub async fn set_route<Sink: PacketSender + 'static>(&self, route: Sink) {
        let sink: PacketSink = Box::new(route);
        _ = self
            .router
            .routes
            .write()
            .await
            .insert(self.addr, sink.into());
    }
}

impl<S: PacketSender + 'static> Drop for IpLease<S> {
    fn drop(&mut self) {
        let addr = self.addr;
        let router = self.router.clone();
        tokio::spawn(async move {
            let route = router.routes.write().await.remove(&addr);
            if let Some(sink) = route {
                if let Err(e) = sink.lock().await.close_dyn().await {
                    warn!("could not close stream to {addr}: {e}");
                }
            }
            router.ip_manager.lock().await.release(addr);
        });
    }
}
