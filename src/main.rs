mod ui;

use std::io;

use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;

const USAGE: &str = "\
Usage: openrgb-tui [host[:port]]

Connects to an OpenRGB SDK server (default 127.0.0.1:6742, or the address
saved from the last run). Press ? inside the app for keys.";

fn main() -> io::Result<()> {
    let server = std::env::args().nth(1);
    if matches!(server.as_deref(), Some("-h" | "--help")) {
        println!("{USAGE}");
        return Ok(());
    }

    let mut app = ui::App::new(server);
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = app.run(&mut terminal);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();

    if let Err(e) = app.settings.save() {
        eprintln!("could not save settings: {e}");
    }
    result
}
