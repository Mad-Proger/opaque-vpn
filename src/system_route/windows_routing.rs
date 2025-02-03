use anyhow::{bail, ensure, Context};
use log::error;
use std::{
    alloc::{alloc, dealloc, Layout},
    net::Ipv4Addr,
    ptr::null_mut,
};
use winapi::{
    shared::{
        ipmib::{MIB_IPFORWARDROW, MIB_IPFORWARDTABLE, MIB_IPROUTE_TYPE_INDIRECT},
        minwindef::DWORD,
        nldef::MIB_IPPROTO_NETMGMT,
        winerror::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR},
    },
    um::{
        iphlpapi::{
            CreateIpForwardEntry, DeleteIpForwardEntry, GetIpForwardTable, SetIpForwardEntry,
        },
        winsock2::htonl,
    },
};

struct RouteStorage {
    start: *mut u8,
    offset: isize,
    layout: Layout,
}

impl RouteStorage {
    fn query_routes() -> anyhow::Result<Self> {
        let mut table_size = Self::get_table_size()?;
        let size_field = Layout::new::<DWORD>();
        let array_field =
            Layout::from_size_align(table_size as usize, align_of::<MIB_IPFORWARDROW>())
                .context("could not create routing table memory layout")?;
        let (layout, offset) = size_field.extend(array_field)?;
        let routing_table = unsafe { alloc(layout) };

        let err_code = unsafe {
            GetIpForwardTable(routing_table as *mut MIB_IPFORWARDTABLE, &mut table_size, 1)
        };
        ensure!(
            err_code == NO_ERROR,
            "could not query routing table: {}",
            err_code
        );

        Ok(Self {
            start: routing_table,
            offset: offset as isize,
            layout,
        })
    }

    fn get_table_size() -> anyhow::Result<u32> {
        let mut required_size = 0;
        let err_code = unsafe { GetIpForwardTable(null_mut(), &mut required_size, 0) };
        ensure!(
            err_code == ERROR_INSUFFICIENT_BUFFER,
            "could not query buffer size: {}",
            err_code
        );
        Ok(required_size)
    }

    fn as_slice(&self) -> &[MIB_IPFORWARDROW] {
        let cnt_rows = unsafe { *(self.start as *const DWORD) } as usize;
        unsafe {
            std::slice::from_raw_parts(
                self.start.offset(self.offset) as *const MIB_IPFORWARDROW,
                cnt_rows,
            )
        }
    }
}

impl std::ops::Drop for RouteStorage {
    fn drop(&mut self) {
        unsafe {
            dealloc(self.start, self.layout);
        }
    }
}

pub struct DefaultRoute {
    original: MIB_IPFORWARDROW,
    new_gateway: Option<MIB_IPFORWARDROW>,
}

impl DefaultRoute {
    pub fn try_new() -> anyhow::Result<Self> {
        let routing_table = RouteStorage::query_routes()?;
        let default_route = routing_table
            .as_slice()
            .iter()
            .find(|route| route.dwForwardDest == 0 && route.dwForwardMask == 0)
            .context("could not find default route")
            .cloned()?;
        Ok(Self {
            original: default_route,
            new_gateway: None,
        })
    }

    pub fn reset(&mut self) -> anyhow::Result<()> {
        let Some(mut route) = self.new_gateway.take() else {
            return Ok(());
        };

        let err_code = unsafe { DeleteIpForwardEntry(&mut route) };
        if err_code != NO_ERROR {
            self.new_gateway = Some(route);
            bail!("could not delete new route: {}", err_code);
        }

        let err_code = unsafe { SetIpForwardEntry(&mut self.original) };
        ensure!(
            err_code == NO_ERROR,
            "could not restore default route: {}",
            err_code
        );
        Ok(())
    }

    pub fn reroute(&mut self, gateway: Ipv4Addr, preserved: Ipv4Addr) -> anyhow::Result<()> {
        self.reset()?;

        let mut gateway_route = MIB_IPFORWARDROW {
            dwForwardDest: unsafe { htonl(preserved.to_bits()) },
            dwForwardMask: u32::MAX,
            dwForwardPolicy: 0,
            dwForwardNextHop: self.original.dwForwardNextHop,
            dwForwardIfIndex: self.original.dwForwardIfIndex,
            ForwardType: MIB_IPROUTE_TYPE_INDIRECT,
            ForwardProto: MIB_IPPROTO_NETMGMT,
            dwForwardAge: 0,
            dwForwardNextHopAS: 0,
            dwForwardMetric1: 0,
            dwForwardMetric2: u32::MAX,
            dwForwardMetric3: u32::MAX,
            dwForwardMetric4: u32::MAX,
            dwForwardMetric5: u32::MAX,
        };
        let err_code = unsafe { CreateIpForwardEntry(&mut gateway_route) };
        ensure!(
            err_code == NO_ERROR,
            "could not create new route: {}",
            err_code
        );
        self.new_gateway = Some(gateway_route);

        let mut new_default = self.original;
        new_default.dwForwardNextHop = unsafe { htonl(gateway.to_bits()) };
        let err_code = unsafe { SetIpForwardEntry(&mut new_default) };
        ensure!(
            err_code == NO_ERROR,
            "could not update default route: {}",
            err_code
        );

        Ok(())
    }
}

impl std::ops::Drop for DefaultRoute {
    fn drop(&mut self) {
        if let Err(err) = self.reset() {
            error!(
                "Incorrect route drop: {}. If internet access is messed up, reboot your pc",
                err
            );
        }
    }
}
