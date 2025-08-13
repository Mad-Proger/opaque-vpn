/*
* Opaque VPN - VPN with main focus on opacity to DPI technologies
* Copyright (C) 2025 Mikhail Zaitsev
*
* This program is free software: you can redistribute it and/or modify
* it under the terms of the GNU General Public License as published by
* the Free Software Foundation, either version 3 of the License, or
* (at your option) any later version.
*/

mod dyn_compat;
mod tagged;
mod traits;
mod tun;
mod util;

pub use dyn_compat::DynPacketSender;
pub use tagged::{TaggedPacketReceiver, TaggedPacketSender};
pub use traits::{PacketReceiver, PacketSender};
pub use tun::{TunReceiver, TunSender};
