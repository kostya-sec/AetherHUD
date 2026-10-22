//! Pure window geometry for the HUD shell: off-screen docking, single-monitor
//! clamping and CSS-pixel → physical-pixel conversion.
//!
//! Everything here works in *logical* (DIP) coordinates unless a name says
//! `Physical`. Multi-monitor desktops legitimately have negative origins, so every
//! coordinate is signed.

/// Gap below the monitor edge while docked, so no pixel of the transparent window
/// can ever peek out at the bottom of the screen.
pub const DOCK_GAP: f64 = 1.0;

/// One monitor's visible frame in logical coordinates plus its DPI scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale_factor: f64,
}

impl MonitorBounds {
    pub fn new(x: f64, y: f64, width: f64, height: f64, scale_factor: f64) -> Self {
        Self {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
            scale_factor: if scale_factor > 0.0 {
                scale_factor
            } else {
                1.0
            },
        }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Whether a logical point lies inside the monitor (half-open on the far edges).
    pub fn contains(&self, point: (f64, f64)) -> bool {
        point.0 >= self.x && point.0 < self.right() && point.1 >= self.y && point.1 < self.bottom()
    }
}

/// A rectangle in logical coordinates. The HUD webview reports hitboxes in CSS
/// pixels relative to the window, which are logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl LogicalRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width: width.max(0.0),
            height: height.max(0.0),
        }
    }
}

/// A rectangle in physical screen coordinates (what the OS reports for the cursor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl PhysicalRect {
    pub fn contains(&self, point: (i32, i32)) -> bool {
        let right = self.x.saturating_add(self.width);
        let bottom = self.y.saturating_add(self.height);
        point.0 >= self.x && point.0 < right && point.1 >= self.y && point.1 < bottom
    }
}

/// Clamps a window position so the window stays inside one monitor.
///
/// A window larger than the monitor is pinned to the monitor's origin instead of
/// being pushed off its far edge.
pub fn clamp_to_monitor(
    position: (f64, f64),
    size: (f64, f64),
    bounds: &MonitorBounds,
) -> (f64, f64) {
    let max_x = (bounds.right() - size.0).max(bounds.x);
    let max_y = (bounds.bottom() - size.1).max(bounds.y);
    (
        position.0.clamp(bounds.x, max_x),
        position.1.clamp(bounds.y, max_y),
    )
}

/// Centers the window in the monitor, respecting the same clamping rules.
pub fn centered_in(size: (f64, f64), bounds: &MonitorBounds) -> (f64, f64) {
    clamp_to_monitor(
        (
            bounds.x + (bounds.width - size.0) / 2.0,
            bounds.y + (bounds.height - size.1) / 2.0,
        ),
        size,
        bounds,
    )
}

/// Off-screen parking spot: same x as the on-screen target (so the HUD slides
/// straight up/down) and just below the monitor's bottom edge.
pub fn dock_position(target: (f64, f64), bounds: &MonitorBounds) -> (f64, f64) {
    (target.0, bounds.bottom() + DOCK_GAP)
}

/// Where the HUD should end up when summoned: last visible position if it still
/// fits this monitor, otherwise centered.
pub fn summon_target(
    last_visible: Option<(f64, f64)>,
    size: (f64, f64),
    bounds: &MonitorBounds,
) -> (f64, f64) {
    match last_visible {
        Some(position) => clamp_to_monitor(position, size, bounds),
        None => centered_in(size, bounds),
    }
}

/// Picks the monitor that owns a point, falling back to the nearest monitor so a
/// stale remembered position can never leave the HUD without a target screen.
pub fn monitor_for_point(monitors: &[MonitorBounds], point: (f64, f64)) -> Option<usize> {
    if monitors.is_empty() {
        return None;
    }
    if let Some(index) = monitors.iter().position(|bounds| bounds.contains(point)) {
        return Some(index);
    }
    monitors
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            distance_to_monitor(left, point)
                .partial_cmp(&distance_to_monitor(right, point))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
}

fn distance_to_monitor(bounds: &MonitorBounds, point: (f64, f64)) -> f64 {
    let dx = if point.0 < bounds.x {
        bounds.x - point.0
    } else if point.0 > bounds.right() {
        point.0 - bounds.right()
    } else {
        0.0
    };
    let dy = if point.1 < bounds.y {
        bounds.y - point.1
    } else if point.1 > bounds.bottom() {
        point.1 - bounds.bottom()
    } else {
        0.0
    };
    dx.hypot(dy)
}

/// Logical coordinate for a physical pixel.
pub fn to_logical(physical: (i32, i32), scale_factor: f64) -> (f64, f64) {
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    (physical.0 as f64 / scale, physical.1 as f64 / scale)
}

/// Converts a window-relative CSS rectangle into a physical screen rectangle.
pub fn hitbox_to_physical(
    window_origin: (i32, i32),
    scale_factor: f64,
    rect: LogicalRect,
) -> PhysicalRect {
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    PhysicalRect {
        x: window_origin
            .0
            .saturating_add((rect.x * scale).round() as i32),
        y: window_origin
            .1
            .saturating_add((rect.y * scale).round() as i32),
        width: (rect.width * scale).round() as i32,
        height: (rect.height * scale).round() as i32,
    }
}

/// Normalized 0..=1 progress for a slide animation.
pub fn progress(elapsed_ms: f64, duration_ms: f64) -> f64 {
    if duration_ms <= 0.0 {
        return 1.0;
    }
    (elapsed_ms / duration_ms).clamp(0.0, 1.0)
}

/// Ease-out cubic: fast first frame, soft landing (the HUD decelerates into place).
pub fn ease_out_cubic(progress: f64) -> f64 {
    let inverted = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - inverted * inverted * inverted
}

/// Linear interpolation between two logical positions.
pub fn interpolate(from: (f64, f64), to: (f64, f64), eased: f64) -> (f64, f64) {
    (
        from.0 + (to.0 - from.0) * eased,
        from.1 + (to.1 - from.1) * eased,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary() -> MonitorBounds {
        MonitorBounds::new(0.0, 0.0, 1920.0, 1080.0, 1.0)
    }

    fn left_of_primary() -> MonitorBounds {
        // A monitor to the left of the primary one: negative logical origin.
        MonitorBounds::new(-1920.0, -200.0, 1920.0, 1080.0, 1.5)
    }

    #[test]
    fn position_is_clamped_into_a_monitor_with_negative_origin() {
        let clamped = clamp_to_monitor((-2500.0, -500.0), (1080.0, 720.0), &left_of_primary());
        assert_eq!(clamped, (-1920.0, -200.0));
    }

    #[test]
    fn position_is_clamped_back_from_the_far_edges() {
        let clamped = clamp_to_monitor((5000.0, 5000.0), (1080.0, 720.0), &primary());
        assert_eq!(clamped, (840.0, 360.0));
    }

    #[test]
    fn a_window_larger_than_the_monitor_is_pinned_to_its_origin() {
        let small = MonitorBounds::new(-100.0, -100.0, 800.0, 600.0, 1.0);
        let clamped = clamp_to_monitor((50.0, 50.0), (1200.0, 900.0), &small);
        assert_eq!(clamped, (-100.0, -100.0));
    }

    #[test]
    fn centering_clamps_to_the_same_single_monitor() {
        assert_eq!(centered_in((1080.0, 720.0), &primary()), (420.0, 180.0));
    }

    #[test]
    fn docking_parks_the_window_fully_below_the_monitor() {
        let target = (420.0, 180.0);
        let docked = dock_position(target, &primary());
        assert_eq!(docked.0, target.0, "the slide must stay vertical");
        assert!(
            docked.1 >= primary().bottom(),
            "docked window must start at or below the monitor bottom edge"
        );
        assert_eq!(docked.1, 1080.0 + DOCK_GAP);
    }

    #[test]
    fn docking_works_on_a_monitor_above_the_primary_one() {
        let upper = MonitorBounds::new(0.0, -1080.0, 1920.0, 1080.0, 1.0);
        assert_eq!(dock_position((10.0, -1000.0), &upper).1, DOCK_GAP);
    }

    #[test]
    fn summoning_prefers_the_last_visible_position_and_clamps_it() {
        assert_eq!(
            summon_target(None, (1080.0, 720.0), &primary()),
            centered_in((1080.0, 720.0), &primary())
        );
        assert_eq!(
            summon_target(Some((100.0, 120.0)), (1080.0, 720.0), &primary()),
            (100.0, 120.0)
        );
        assert_eq!(
            summon_target(Some((-5000.0, 5000.0)), (1080.0, 720.0), &primary()),
            (0.0, 360.0)
        );
    }

    #[test]
    fn monitor_lookup_prefers_the_containing_monitor() {
        let monitors = [left_of_primary(), primary()];
        assert_eq!(monitor_for_point(&monitors, (100.0, 100.0)), Some(1));
        assert_eq!(monitor_for_point(&monitors, (-1000.0, 100.0)), Some(0));
    }

    #[test]
    fn monitor_lookup_falls_back_to_the_nearest_monitor() {
        let monitors = [left_of_primary(), primary()];
        // Off every monitor, but closer to the left one.
        assert_eq!(monitor_for_point(&monitors, (-3000.0, 0.0)), Some(0));
        assert_eq!(monitor_for_point(&monitors, (3000.0, 0.0)), Some(1));
        assert_eq!(monitor_for_point(&[], (0.0, 0.0)), None);
    }

    #[test]
    fn logical_conversion_round_trips_and_rounds_to_pixels() {
        assert_eq!(to_logical((21, 41), 2.0), (10.5, 20.5));
        // A zero scale factor must never produce NaN/division by zero.
        assert_eq!(to_logical((5, 5), 0.0), (5.0, 5.0));
    }

    #[test]
    fn hitboxes_convert_with_negative_window_origins() {
        let rect = LogicalRect::new(10.0, 20.0, 100.0, 40.0);
        let physical = hitbox_to_physical((-1920, 100), 1.5, rect);
        assert_eq!(
            physical,
            PhysicalRect {
                x: -1905,
                y: 130,
                width: 150,
                height: 60
            }
        );
        assert!(physical.contains((-1905, 130)));
        assert!(physical.contains((-1756, 189)));
        assert!(!physical.contains((-1755, 189)));
        assert!(!physical.contains((-1906, 130)));
    }

    #[test]
    fn slide_progress_is_clamped_and_eased() {
        assert_eq!(progress(-10.0, 180.0), 0.0);
        assert_eq!(progress(90.0, 180.0), 0.5);
        assert_eq!(progress(500.0, 180.0), 1.0);
        assert_eq!(progress(10.0, 0.0), 1.0);

        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        assert!(ease_out_cubic(0.5) > 0.5, "ease-out lands early");
        assert!(ease_out_cubic(0.25) < ease_out_cubic(0.75));

        assert_eq!(interpolate((0.0, 100.0), (0.0, 200.0), 0.25), (0.0, 125.0));
    }
}
