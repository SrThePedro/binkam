//! binkam — an animated, clock-centered terminal screensaver.

mod anim;
mod app;
mod cli;
mod clock;
mod color;
mod config;
mod render;
mod theme;

use std::io;

use clap::Parser;
use ratatui::crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
};

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

    // Headless paths: neither enters raw mode or the alternate screen.
    if let Some(frames) = cli.bench {
        return app::bench(&cli, &config, frames);
    }
    if cli.dump_frame {
        return app::dump_frame(&cli, &config);
    }

    // `ratatui::init` enables raw mode, switches to the alternate screen and
    // installs a panic hook that restores the terminal before printing.
    let terminal = ratatui::init();
    guard_mouse_capture();
    let result = start_mouse_capture()
        .and_then(|()| App::new(&cli, &config))
        .and_then(|app| app.run(terminal));

    // Both run on the error path too, because `result` is returned after.
    stop_mouse_capture();
    ratatui::restore();
    result
}

/// Ask the terminal to report mouse movement and clicks.
fn start_mouse_capture() -> color_eyre::Result<()> {
    execute!(io::stdout(), EnableMouseCapture)?;
    Ok(())
}

/// Stop it again. Errors are ignored: we are on our way out either way, and
/// a terminal that will not listen cannot be put right by complaining.
fn stop_mouse_capture() {
    let _ = execute!(io::stdout(), DisableMouseCapture);
}

/// Make sure a panic turns mouse capture off as well.
///
/// `ratatui::init` already installed a hook that restores the screen, but it
/// knows nothing about mouse capture; left on, the terminal would keep
/// spraying escape codes at the shell. This one runs first and then hands
/// over to it.
fn guard_mouse_capture() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        stop_mouse_capture();
        previous(info);
    }));
}
