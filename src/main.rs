#![feature(addr_parse_ascii)]
mod config;

use config::load_config;

fn main() -> anyhow::Result<()> {
    let config = load_config(std::env::args().nth(1).expect("no config file provided"))?;
    Ok(())
}
