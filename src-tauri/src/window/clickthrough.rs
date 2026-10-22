//! Whole-window click-through with hitbox polling (#24 §8).
//!
//! The HUD is a full-screen-ish transparent window, so the OS must ignore the cursor
//! over empty space and only capture it over interactive controls. The webview reports
//! hitboxes in CSS pixels; the shell converts them to physical screen coordinates on
//! every poll (the window moves, the DPI can change) and flips
//! `set_ignore_cursor_events` only when the answer changes.

use std::sync::Mutex;
use std::time::Duration;

use super::geometry::{hitbox_to_physical, LogicalRect, PhysicalRect};

/// Cursor polling period. Fast enough that hovering a control feels immediate and
/// slow enough to stay invisible in the CPU profile.
pub const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The hitboxes reported by the webview for the current layout.
#[derive(Debug, Default)]
pub struct HitBoxRegistry {
    rects: Mutex<Vec<LogicalRect>>,
}

impl HitBoxRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the whole set. The webview sends a complete list, never a delta.
    pub fn replace(&self, rects: Vec<LogicalRect>) {
        let mut current = self
            .rects
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *current = rects;
    }

    /// Physical hitboxes for the window's current position and DPI.
    pub fn physical(&self, window_origin: (i32, i32), scale_factor: f64) -> Vec<PhysicalRect> {
        let current = self
            .rects
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        current
            .iter()
            .map(|rect| hitbox_to_physical(window_origin, scale_factor, *rect))
            .collect()
    }

    /// Whether a physical cursor position is over any reported hitbox.
    pub fn hits(&self, window_origin: (i32, i32), scale_factor: f64, point: (i32, i32)) -> bool {
        self.physical(window_origin, scale_factor)
            .iter()
            .any(|rect| rect.contains(point))
    }
}

/// Mirror of the window's `ignore_cursor_events` flag. Keeps the shell from calling
/// the OS setter on every 50 ms tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IgnoreCursor {
    ignoring: bool,
}

impl Default for IgnoreCursor {
    fn default() -> Self {
        // The HUD starts fully click-through; the webview reports hitboxes once it has
        // rendered, and only then can a click be captured.
        Self { ignoring: true }
    }
}

impl IgnoreCursor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies a hit-test result and returns the new OS value when it must change.
    pub fn update(&mut self, hit: bool) -> Option<bool> {
        let wanted = !hit;
        if wanted == self.ignoring {
            return None;
        }
        self.ignoring = wanted;
        Some(wanted)
    }

    /// Forces click-through (used while docked off screen and while dismissing).
    pub fn force_ignore(&mut self) -> Option<bool> {
        self.update(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(rects: Vec<LogicalRect>) -> HitBoxRegistry {
        let registry = HitBoxRegistry::new();
        registry.replace(rects);
        registry
    }

    #[test]
    fn hit_testing_respects_the_window_origin_and_dpi_scale() {
        // Window parked on a monitor left of the primary one; 150% DPI.
        let registry = registry_with(vec![LogicalRect::new(10.0, 20.0, 100.0, 40.0)]);

        assert!(registry.hits((-1920, 100), 1.5, (-1905, 130)));
        assert!(registry.hits((-1920, 100), 1.5, (-1756, 189)));
        assert!(!registry.hits((-1920, 100), 1.5, (-1906, 130)));
        assert!(!registry.hits((-1920, 100), 1.5, (-1755, 189)));
    }

    #[test]
    fn a_shot_hitbox_accepts_only_its_interior() {
        let registry = registry_with(vec![LogicalRect::new(0.0, 0.0, 10.0, 10.0)]);
        assert!(registry.hits((0, 0), 1.0, (0, 0)));
        assert!(!registry.hits((0, 0), 1.0, (10, 10)));
        assert!(!registry.hits((0, 0), 1.0, (-1, 5)));
    }

    #[test]
    fn an_empty_hitbox_list_never_captures_the_cursor() {
        let registry = registry_with(Vec::new());
        assert!(registry.physical((0, 0), 1.0).is_empty());
        assert!(!registry.hits((0, 0), 1.0, (0, 0)));
    }

    #[test]
    fn replacing_the_hitboxes_drops_the_previous_layout() {
        let registry = registry_with(vec![LogicalRect::new(0.0, 0.0, 10.0, 10.0)]);
        registry.replace(vec![LogicalRect::new(100.0, 100.0, 10.0, 10.0)]);

        assert!(!registry.hits((0, 0), 1.0, (5, 5)));
        assert!(registry.hits((0, 0), 1.0, (105, 105)));
    }

    #[test]
    fn the_ignore_flag_only_flips_when_the_answer_changes() {
        let mut ignore = IgnoreCursor::new();
        assert_eq!(ignore.update(false), None, "already click-through");

        assert_eq!(
            ignore.update(true),
            Some(false),
            "hovering a control captures"
        );
        assert_eq!(ignore.update(true), None);

        assert_eq!(
            ignore.update(false),
            Some(true),
            "leaving the control passes through"
        );
        assert_eq!(ignore.update(false), None);
    }

    #[test]
    fn forcing_click_through_is_idempotent() {
        let mut ignore = IgnoreCursor::new();
        assert_eq!(ignore.force_ignore(), None);

        ignore.update(true);
        assert_eq!(ignore.force_ignore(), Some(true));
        assert_eq!(ignore.force_ignore(), None);
    }
}
