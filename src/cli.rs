//! Command line interface definition.

use clap::Parser;

/// An animated, clock-centered terminal screensaver.
#[derive(Debug, Parser)]
#[command(name = "kumsaati", version, about)]
pub struct Cli {
    /// Target frames per second for the render loop.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u16).range(1..=240))]
    pub fps: u16,

    /// Render a single frame to stdout as plain text and exit.
    ///
    /// Uses a headless backend, so the real terminal is never touched.
    #[arg(long)]
    pub dump_frame: bool,

    /// Terminal size used by `--dump-frame`, given as WIDTHxHEIGHT (e.g. 80x24).
    #[arg(long, value_parser = parse_size)]
    pub size: Option<(u16, u16)>,
}

impl Cli {
    /// Size to use for a headless frame dump, falling back to a sane default.
    pub fn dump_size(&self) -> (u16, u16) {
        self.size.unwrap_or((80, 24))
    }

    /// Duration of a single frame derived from [`Cli::fps`].
    pub fn frame_time(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(1.0 / f64::from(self.fps))
    }
}

/// Parse a `WIDTHxHEIGHT` string such as `80x24`.
fn parse_size(raw: &str) -> Result<(u16, u16), String> {
    let (w, h) = raw
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected WIDTHxHEIGHT (e.g. 80x24), got `{raw}`"))?;

    let parse = |part: &str, what: &str| -> Result<u16, String> {
        part.trim()
            .parse::<u16>()
            .map_err(|e| format!("invalid {what} `{part}`: {e}"))
            .and_then(|v| {
                if v == 0 {
                    Err(format!("{what} must be greater than zero"))
                } else {
                    Ok(v)
                }
            })
    };

    Ok((parse(w, "width")?, parse(h, "height")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_sizes() {
        assert_eq!(parse_size("80x24").unwrap(), (80, 24));
        assert_eq!(parse_size("200X50").unwrap(), (200, 50));
    }

    #[test]
    fn rejects_malformed_sizes() {
        for bad in ["80", "80x", "x24", "0x24", "80x0", "axb", "-1x10"] {
            assert!(parse_size(bad).is_err(), "`{bad}` should not parse");
        }
    }

    #[test]
    fn frame_time_matches_fps() {
        let cli = Cli::parse_from(["kumsaati", "--fps", "50"]);
        assert_eq!(cli.frame_time(), std::time::Duration::from_millis(20));
    }

    #[test]
    fn dump_size_defaults_to_80x24() {
        let cli = Cli::parse_from(["kumsaati"]);
        assert_eq!(cli.dump_size(), (80, 24));
    }

    #[test]
    fn verifies_cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
