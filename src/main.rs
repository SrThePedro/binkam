//! kumsaati — an animated, clock-centered terminal screensaver.

mod anim;
mod app;
mod cli;
mod clock;
mod render;

use clap::Parser;

use crate::{app::App, cli::Cli};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    if cli.list {
        for name in anim::all_names() {
            println!("{name}");
        }
        return Ok(());
    }

    // Headless path: never enters raw mode or the alternate screen.
    if cli.dump_frame {
        return app::dump_frame(&cli);
    }

    // `ratatui::init` enables raw mode, switches to the alternate screen and
    // installs a panic hook that restores the terminal before printing.
    let terminal = ratatui::init();
    let result = App::new(&cli).and_then(|app| app.run(terminal));
    // Runs on the error path too, because `result` is returned afterwards.
    ratatui::restore();
    result
}
