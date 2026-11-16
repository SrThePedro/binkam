//! Background animations and the registry that builds them by name.

pub mod fire;
pub mod matrix;
pub mod starfield;

use std::time::Duration;

use ratatui::{buffer::Buffer, layout::Rect};

/// A full-screen background effect.
///
/// Simulation and drawing are deliberately split: [`Animation::update`] may
/// change state but draws nothing, [`Animation::render`] draws but takes
/// `&self`, so the compiler guarantees a frame can never mutate the world.
pub trait Animation {
    /// The name this animation is registered under.
    fn name(&self) -> &'static str;

    /// Called on the first frame and whenever the terminal is resized.
    fn resize(&mut self, area: Rect);

    /// Advance the simulation by `dt`. No drawing here.
    fn update(&mut self, dt: Duration);

    /// Draw the current state. Must not allocate.
    fn render(&self, area: Rect, buf: &mut Buffer);
}

/// Every animation name, in the order they are offered to the user.
pub const fn all_names() -> &'static [&'static str] {
    &["fire", "matrix", "starfield"]
}

/// Build the animation called `name`, seeded with `seed`.
///
/// Returns `None` for an unknown name. The result is boxed because each
/// animation is a different concrete type: `Box<dyn Animation>` stores any of
/// them behind one pointer and dispatches calls through a vtable.
pub fn by_name(name: &str, seed: u64) -> Option<Box<dyn Animation>> {
    match name {
        "fire" => Some(Box::new(fire::Fire::new(seed))),
        "matrix" => Some(Box::new(matrix::Matrix::new(seed))),
        "starfield" => Some(Box::new(starfield::Starfield::new(seed))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_can_be_built() {
        for name in all_names() {
            let animation = by_name(name, 1).unwrap_or_else(|| panic!("{name} is not buildable"));
            assert_eq!(
                &animation.name(),
                name,
                "name() must match the registry key"
            );
        }
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert!(by_name("definitely-not-an-animation", 1).is_none());
    }

    #[test]
    fn names_are_sorted_and_unique() {
        let names = all_names();
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted, names,
            "keep all_names sorted and free of duplicates"
        );
    }
}
