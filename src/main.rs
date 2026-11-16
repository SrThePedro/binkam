//! kumsaati — an animated, clock-centered terminal screensaver.

mod anim;
mod app;
mod cli;
mod clock;
mod color;
mod config;
mod render;
mod theme;

use clap::Parser;

use crate::{app::App, cli::Cli, config::Config};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    if cli.list {
        for name in anim::all_names() {
            println!("{name}");
        }
        return Ok(());
    }

    if cli.list_themes {
        for name in theme::Theme::all_names() {
            println!("{name}");
        }
        return Ok(());
    }

    // Settings: the defaults, under the config file, under the command line.
    let config = Config::resolve(&cli)?;

    if cli.print_config {
        print!("{}", config.to_toml()?);
        return Ok(());
    }

    // Headless path: never enters raw mode or the alternate screen.
    if cli.dump_frame {
        return app::dump_frame(&cli, &config);
    }

    // `ratatui::init` enables raw mode, switches to the alternate screen and
    // installs a panic hook that restores the terminal before printing.
    let terminal = ratatui::init();
    let result = App::new(&cli, &config).and_then(|app| app.run(terminal));
    // Runs on the error path too, because `result` is returned afterwards.
    ratatui::restore();
    result
}
