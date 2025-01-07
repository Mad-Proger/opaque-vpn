use std::{collections::BTreeSet, net::Ipv4Addr};

pub struct IpManager {
    blocked: BTreeSet<u32>,
    subnet: u32,
    netmask: u32,
    min_free: u32,
    subnet_size: u32,
}

impl IpManager {
    pub fn new(subnet: Ipv4Addr, netmask: Ipv4Addr) -> Self {
        let netmask_bits = netmask.to_bits();
        let subnet_size = 1u32 << netmask_bits.count_zeros();
        let subnet_bits = subnet.to_bits() & netmask_bits;
        Self {
            blocked: BTreeSet::new(),
            subnet: subnet_bits,
            netmask: netmask_bits,
            min_free: 0,
            subnet_size,
        }
    }

    pub fn block(&mut self, addr: Ipv4Addr) {
        let addr_bits = addr.to_bits();
        if (addr_bits & self.netmask) != self.subnet {
            return;
        }

        let to_block = self.compress_address(addr_bits);
        self.blocked.insert(to_block);
        while self.blocked.contains(&self.min_free) {
            self.min_free += 1;
        }
    }

    pub fn release(&mut self, addr: Ipv4Addr) {
        let addr_bits = addr.to_bits();
        if (addr_bits & self.netmask) != self.subnet {
            return;
        }

        let to_unblock = self.compress_address(addr_bits);
        if self.blocked.remove(&to_unblock) && to_unblock < self.min_free {
            self.min_free = to_unblock;
        }
    }

    pub fn get_free(&self) -> Option<Ipv4Addr> {
        if self.min_free < self.subnet_size {
            Some(self.expand_bits(self.min_free))
        } else {
            None
        }
    }

    fn compress_address(&self, addr_bits: u32) -> u32 {
        let mut mask = !self.subnet;
        let mut offset = 1u32;
        let mut res = 0;
        while mask != 0 {
            let lowest_bit = mask & !(mask - 1);
            if addr_bits & lowest_bit != 0 {
                res |= offset;
            }
            mask ^= lowest_bit;
            offset <<= 1;
        }
        res
    }

    fn expand_bits(&self, bits: u32) -> Ipv4Addr {
        let mut addr_bits = 0;
        let mut mask = !self.subnet;
        let mut offset = 1u32;
        while mask != 0 {
            let lowest_bit = mask & !(mask - 1);
            if bits & offset != 0 {
                addr_bits |= lowest_bit;
            }
            mask ^= lowest_bit;
            offset <<= 1;
        }
        Ipv4Addr::from_bits(self.subnet | addr_bits)
    }
}
