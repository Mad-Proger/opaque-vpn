/*
* Opaque VPN - VPN with main focus on opacity to DPI technologies
* Copyright (C) 2025 Mikhail Zaitsev
*
* This program is free software: you can redistribute it and/or modify
* it under the terms of the GNU General Public License as published by
* the Free Software Foundation, either version 3 of the License, or
* (at your option) any later version.
*/

use std::pin::Pin;

use futures::{future::Future, io};

use crate::packet_stream::PacketSender;

pub trait DynPacketSender: Send {
    fn send_dyn<'a>(
        &'a mut self,
        packet: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>>;

    fn close_dyn(&mut self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>>;
}

impl<S: PacketSender> DynPacketSender for S {
    fn send_dyn<'a>(
        &'a mut self,
        packet: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'a>> {
        Box::pin(self.send(packet))
    }

    fn close_dyn(&mut self) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(self.close())
    }
}
