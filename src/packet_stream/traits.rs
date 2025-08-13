/*
* Opaque VPN - VPN with main focus on opacity to DPI technologies
* Copyright (C) 2025 Mikhail Zaitsev
*
* This program is free software: you can redistribute it and/or modify
* it under the terms of the GNU General Public License as published by
* the Free Software Foundation, either version 3 of the License, or
* (at your option) any later version.
*/

use futures::{future::Future, io};

pub trait PacketReceiver: Send {
    fn receive(&mut self) -> impl Future<Output = io::Result<Box<[u8]>>> + Send;
}

pub trait PacketSender: Send {
    fn send(&mut self, packet: &[u8]) -> impl Future<Output = io::Result<()>> + Send;

    fn close(&mut self) -> impl Future<Output = io::Result<()>> + Send;
}
