//! Read-only diagnostic: lists the devices an OpenRGB server reports.
//!
//! ```sh
//! cargo run --example probe -- 127.0.0.1 6742
//! ```

use openrgb_tui::client::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| "127.0.0.1".into());
    let port = args.next().and_then(|p| p.parse().ok()).unwrap_or(6742);
    let mut c = Client::connect(&host, port, "OpenRGB TUI probe")?;
    println!("server protocol v{}, using v{}", c.server_proto, c.proto);
    for (i, d) in c.controllers()?.iter().enumerate() {
        let mode = d.active_mode().map(|m| m.name.as_str()).unwrap_or("?");
        println!(
            "[{i}] {} — {} ({}) · {} modes, active \"{mode}\" · {} zones · {} LEDs",
            d.name,
            d.vendor,
            d.kind.label(),
            d.modes.len(),
            d.zones.len(),
            d.colors.len()
        );
    }
    println!("profiles: {:?}", c.profiles()?);
    Ok(())
}
