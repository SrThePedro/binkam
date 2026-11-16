//! Where the clock sits, and how it moves.
//!
//! The widget itself always draws in the middle of whatever rectangle it is
//! given. Moving it around is this module's job: it keeps a position inside
//! the screen and hands out a rectangle exactly the size of the clock.

use std::{f32::consts::TAU, time::Duration};

use ratatui::layout::Rect;

/// Columns the bouncing clock covers per second.
const BOUNCE_SPEED_X: f32 = 7.0;
/// Rows per second. Half the horizontal speed, because a cell is about twice
/// as tall as it is wide, so this reads as the same speed on screen.
const BOUNCE_SPEED_Y: f32 = 3.5;

/// Turns of drift phase per second. One full tour takes a few minutes.
const DRIFT_RATE: f32 = 0.0045;
/// Ratio between the two drift axes, so the path is a slow Lissajous curve
/// rather than a straight diagonal.
const DRIFT_RATIO: f32 = 0.7;
/// Drift phase wraps here; the ratio is a tenth, so ten turns is a period
/// both axes share and the path stays continuous across the wrap.
const DRIFT_PERIOD: f32 = 10.0;

/// Gap left between the clock and the screen edge in `corner` mode.
const CORNER_MARGIN_X: f32 = 2.0;
const CORNER_MARGIN_Y: f32 = 1.0;

/// Longest frame gap the motion reacts to.
const MAX_STEP: f32 = 0.1;

/// Where the clock is placed on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockPosition {
    /// Bounces off the edges like a DVD logo, changing colour on each hit.
    Bounce,
    /// Fixed in the middle.
    Center,
    /// Tucked into the bottom right corner.
    Corner,
    /// Wanders slowly, which keeps a static image off one set of pixels.
    Drift,
}

impl ClockPosition {
    /// Every position name, in the order they are offered to the user.
    pub const fn all_names() -> &'static [&'static str] {
        &["bounce", "center", "corner", "drift"]
    }

    /// The position called `name`, or `None` if there is no such position.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "bounce" => Some(Self::Bounce),
            "center" => Some(Self::Center),
            "corner" => Some(Self::Corner),
            "drift" => Some(Self::Drift),
            _ => None,
        }
    }

    /// The name this position is registered under.
    pub fn name(self) -> &'static str {
        match self {
            Self::Bounce => "bounce",
            Self::Center => "center",
            Self::Corner => "corner",
            Self::Drift => "drift",
        }
    }
}

/// Keeps track of where the clock is and moves it.
#[derive(Debug)]
pub struct ClockMotion {
    position: ClockPosition,
    /// The area the clock moves inside.
    area: Rect,
    /// Size of the clock box, as the widget measured it.
    size: (u16, u16),
    /// Top-left of the box, relative to `area`, in cells.
    x: f32,
    y: f32,
    /// Velocity in cells per second, used by `bounce`.
    velocity_x: f32,
    velocity_y: f32,
    /// Drift phase in turns.
    phase: f32,
    /// How many edges the clock has hit, which picks the bounce colour.
    bounces: usize,
    /// Whether the clock has ever been given a screen to sit on. Until it
    /// has, `bounce` has no sensible position to carry over.
    placed: bool,
}

impl ClockMotion {
    /// A motion in the given mode, not yet given a screen.
    pub fn new(position: ClockPosition) -> Self {
        Self {
            position,
            area: Rect::ZERO,
            size: (0, 0),
            x: 0.0,
            y: 0.0,
            velocity_x: BOUNCE_SPEED_X,
            velocity_y: BOUNCE_SPEED_Y,
            phase: 0.0,
            bounces: 0,
            placed: false,
        }
    }

    /// The mode currently in force.
    pub fn position(&self) -> ClockPosition {
        self.position
    }

    /// Switch mode, carrying the current position over as a starting point.
    pub fn set_position(&mut self, position: ClockPosition) {
        self.position = position;
        self.settle();
    }

    /// How many edges have been hit so far.
    pub fn bounces(&self) -> usize {
        self.bounces
    }

    /// Tell the motion about the screen and the size of the clock on it.
    pub fn set_bounds(&mut self, area: Rect, size: (u16, u16)) {
        if self.area == area && self.size == size {
            return;
        }
        self.area = area;
        self.size = size;
        self.settle();
    }

    /// Columns and rows of free space around the clock.
    fn span(&self) -> (f32, f32) {
        (
            f32::from(self.area.width.saturating_sub(self.size.0)),
            f32::from(self.area.height.saturating_sub(self.size.1)),
        )
    }

    /// Put the clock somewhere valid for the current mode and bounds.
    fn settle(&mut self) {
        let (span_x, span_y) = self.span();
        match self.position {
            ClockPosition::Center | ClockPosition::Drift => {
                self.x = span_x / 2.0;
                self.y = span_y / 2.0;
            }
            ClockPosition::Corner => {
                self.x = (span_x - CORNER_MARGIN_X).max(0.0);
                self.y = (span_y - CORNER_MARGIN_Y).max(0.0);
            }
            // Bounce carries on from wherever it was, just inside the new
            // edges. With nowhere to carry over from, it starts in the middle
            // rather than wedged in a corner.
            ClockPosition::Bounce if self.placed => {
                self.x = self.x.clamp(0.0, span_x);
                self.y = self.y.clamp(0.0, span_y);
            }
            ClockPosition::Bounce => {
                self.x = span_x / 2.0;
                self.y = span_y / 2.0;
            }
        }
        self.placed = true;
    }

    /// Move the clock on by `dt`.
    pub fn update(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32().min(MAX_STEP);
        let (span_x, span_y) = self.span();

        match self.position {
            // Static modes are already where they belong; `settle` put them
            // there and only a resize can move them.
            ClockPosition::Center | ClockPosition::Corner => {}

            ClockPosition::Bounce => {
                let mut bounces = 0;
                bounce(&mut self.x, &mut self.velocity_x, span_x, dt, &mut bounces);
                bounce(&mut self.y, &mut self.velocity_y, span_y, dt, &mut bounces);
                self.bounces += bounces;
            }

            ClockPosition::Drift => {
                self.phase = (self.phase + DRIFT_RATE * dt).rem_euclid(DRIFT_PERIOD);
                let half_x = span_x / 2.0;
                let half_y = span_y / 2.0;
                self.x = half_x + half_x * (self.phase * TAU).sin();
                self.y = half_y + half_y * (self.phase * DRIFT_RATIO * TAU).sin();
            }
        }
    }

    /// The rectangle to draw the clock in.
    pub fn rect(&self) -> Rect {
        let (span_x, span_y) = self.span();
        Rect::new(
            self.area.x + self.x.clamp(0.0, span_x) as u16,
            self.area.y + self.y.clamp(0.0, span_y) as u16,
            self.size.0,
            self.size.1,
        )
    }
}

/// Move one axis and reflect it off both ends, counting the hits.
///
/// Reflection mirrors the overshoot rather than pinning the clock to the
/// edge, so the motion stays smooth whatever the frame rate.
fn bounce(position: &mut f32, velocity: &mut f32, span: f32, dt: f32, bounces: &mut usize) {
    if span <= 0.0 {
        // No room to move on this axis; bouncing in place would strobe the
        // colour on every single frame.
        *position = 0.0;
        return;
    }

    *position += *velocity * dt;

    if *position < 0.0 {
        *position = -*position;
        *velocity = velocity.abs();
        *bounces += 1;
    } else if *position > span {
        *position = 2.0 * span - *position;
        *velocity = -velocity.abs();
        *bounces += 1;
    }

    *position = position.clamp(0.0, span);
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 24,
    };
    const SIZE: (u16, u16) = (47, 7);

    fn motion(position: ClockPosition) -> ClockMotion {
        let mut motion = ClockMotion::new(position);
        motion.set_bounds(AREA, SIZE);
        motion
    }

    #[test]
    fn every_listed_position_round_trips() {
        for name in ClockPosition::all_names() {
            let position = ClockPosition::by_name(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(&position.name(), name);
        }
    }

    #[test]
    fn unknown_positions_are_rejected() {
        assert!(ClockPosition::by_name("sideways").is_none());
    }

    #[test]
    fn names_are_sorted_and_unique() {
        let names = ClockPosition::all_names();
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, names);
    }

    #[test]
    fn center_puts_the_clock_in_the_middle() {
        let motion = motion(ClockPosition::Center);
        // 80 - 47 = 33 columns spare, 24 - 7 = 17 rows.
        assert_eq!(motion.rect(), Rect::new(16, 8, 47, 7));
    }

    #[test]
    fn center_does_not_move() {
        let mut motion = motion(ClockPosition::Center);
        let before = motion.rect();
        for _ in 0..600 {
            motion.update(Duration::from_millis(33));
        }
        assert_eq!(motion.rect(), before);
    }

    #[test]
    fn corner_sits_near_the_bottom_right() {
        let motion = motion(ClockPosition::Corner);
        assert_eq!(motion.rect(), Rect::new(31, 16, 47, 7));
    }

    #[test]
    fn bounce_moves_and_counts_its_hits() {
        let mut motion = motion(ClockPosition::Bounce);
        assert_eq!(motion.rect(), Rect::new(16, 8, 47, 7), "starts centred");

        let start = motion.rect();
        for _ in 0..15 {
            motion.update(Duration::from_millis(33));
        }
        assert_ne!(motion.rect(), start, "the clock should have moved");

        // Long enough to cross the screen several times in both directions.
        for _ in 0..900 {
            motion.update(Duration::from_millis(33));
        }
        assert!(motion.bounces() >= 4, "only {} hits", motion.bounces());
    }

    #[test]
    fn bounce_stays_on_screen() {
        let mut motion = motion(ClockPosition::Bounce);
        for _ in 0..3000 {
            motion.update(Duration::from_millis(33));
            let rect = motion.rect();
            assert!(
                AREA.union(rect) == AREA,
                "{rect:?} left the screen {AREA:?}"
            );
        }
    }

    #[test]
    fn bounce_survives_a_screen_with_no_room() {
        let mut motion = ClockMotion::new(ClockPosition::Bounce);
        motion.set_bounds(Rect::new(0, 0, 47, 7), SIZE);
        for _ in 0..100 {
            motion.update(Duration::from_millis(33));
        }
        assert_eq!(motion.rect(), Rect::new(0, 0, 47, 7));
        assert_eq!(motion.bounces(), 0, "a clock with no room must not strobe");
    }

    #[test]
    fn bounce_survives_a_huge_frame_gap() {
        let mut motion = motion(ClockPosition::Bounce);
        motion.update(Duration::from_secs(3600));
        let rect = motion.rect();
        assert!(AREA.union(rect) == AREA, "{rect:?} left the screen");
    }

    #[test]
    fn drift_wanders_without_leaving_the_screen() {
        let mut motion = motion(ClockPosition::Drift);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..12_000 {
            motion.update(Duration::from_millis(33));
            let rect = motion.rect();
            assert!(AREA.union(rect) == AREA, "{rect:?} left the screen");
            seen.insert((rect.x, rect.y));
        }
        assert!(seen.len() > 20, "drift only visited {} spots", seen.len());
    }

    #[test]
    fn drift_phase_stays_bounded() {
        let mut motion = motion(ClockPosition::Drift);
        for _ in 0..50_000 {
            motion.update(Duration::from_millis(33));
        }
        assert!(
            (0.0..DRIFT_PERIOD).contains(&motion.phase),
            "{}",
            motion.phase
        );
    }

    #[test]
    fn switching_mode_repositions_at_once() {
        let mut motion = motion(ClockPosition::Center);
        motion.set_position(ClockPosition::Corner);
        assert_eq!(motion.position(), ClockPosition::Corner);
        assert_eq!(motion.rect(), Rect::new(31, 16, 47, 7));
    }

    #[test]
    fn a_shrinking_screen_pulls_the_clock_back_in() {
        let mut motion = motion(ClockPosition::Bounce);
        for _ in 0..200 {
            motion.update(Duration::from_millis(33));
        }
        let small = Rect::new(0, 0, 50, 9);
        motion.set_bounds(small, SIZE);
        let rect = motion.rect();
        assert!(small.union(rect) == small, "{rect:?} left {small:?}");
    }

    #[test]
    fn an_offset_area_is_respected() {
        let mut motion = ClockMotion::new(ClockPosition::Center);
        let area = Rect::new(10, 5, 80, 24);
        motion.set_bounds(area, SIZE);
        assert_eq!(motion.rect(), Rect::new(26, 13, 47, 7));
    }

    #[test]
    fn a_clock_that_does_not_fit_gets_an_empty_rect() {
        let mut motion = ClockMotion::new(ClockPosition::Center);
        motion.set_bounds(Rect::new(0, 0, 4, 2), (0, 0));
        assert!(motion.rect().is_empty());
    }
}
