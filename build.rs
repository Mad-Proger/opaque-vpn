/*--========================================--*\
    * Author  : NTheme - All rights reserved
    * Created : 08 August 2025, 4:24 AM
    * File    : build.rs
    * Project : opaque-vpn
\*--========================================--*/

fn main() {
    slint_build::compile("ui.slint").unwrap();
}
