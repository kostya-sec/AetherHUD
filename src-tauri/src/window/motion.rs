//! Summon/dismiss state machine for the resident HUD.
//!
//! The HUD is *never* hidden with `hide()`: it stays visible and slides between an
//! off-screen dock and its on-screen target (#24 §8). This module owns only the
//! decision layer — phase, debounce and the blur grace window — so it can be tested
//! without a real window. Timestamps are passed in, never read from the clock here.

use std::time::{Duration, Instant};

/// Slide-in slide-out length. Short enough to feel immediate, long enough to read.
pub const SLIDE_DURATION: Duration = Duration::from_millis(180);

/// Minimum time between two user-triggered transitions. A held hotkey, a double
/// clicked tray icon or a bouncing second-instance callback must not flap the HUD.
pub const TOGGLE_DEBOUNCE: Duration = Duration::from_millis(160);

/// Grace period after losing focus: clicking a menu or the webview's own dialogs can
/// briefly blur the HUD, and macOS reports resign-key before the click lands (#24 §8).
pub const BLUR_GRACE: Duration = Duration::from_millis(250);

/// Where the HUD currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// Parked off screen; no window pixels on the desktop.
    #[default]
    Hidden,
    /// Sliding from the dock to the target; input is already live.
    Showing,
    /// On screen and interactive.
    Visible,
    /// Sliding back to the dock; the webview swallows keys until the next summon.
    Dismissing,
}

impl Phase {
    /// Whether the HUD occupies (or is about to occupy) on-screen space.
    pub fn is_on_screen(self) -> bool {
        matches!(self, Phase::Showing | Phase::Visible)
    }

    /// Whether the shell is currently animating a slide away.
    pub fn is_dismissing(self) -> bool {
        matches!(self, Phase::Dismissing)
    }

    /// Stable identifier used by the `hud_status` IPC payload.
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Hidden => "hidden",
            Phase::Showing => "showing",
            Phase::Visible => "visible",
            Phase::Dismissing => "dismissing",
        }
    }
}

/// A transition the shell must animate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Summon,
    Dismiss,
}

/// Why a dismissal happened. Blur-driven dismissals must not steal focus back from
/// the application the user just clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissReason {
    /// Esc in the HUD.
    Escape,
    /// The global hotkey.
    Hotkey,
    /// A left click on the tray icon.
    Tray,
    /// Window lost focus (after the grace window).
    Blur,
    /// The OS asked to close the window (Alt+F4 / Cmd+W).
    CloseRequested,
}

impl DismissReason {
    /// Only explicit user actions hand focus back to the previously foreground app.
    pub fn restores_previous_app(self) -> bool {
        !matches!(self, DismissReason::Blur)
    }
}

/// Why a summon happened (kept for logs and for symmetric call sites).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummonReason {
    /// First manual launch finished loading.
    Launch,
    /// The global hotkey fired.
    Hotkey,
    /// The tray icon was left-clicked.
    Tray,
    /// A second process handed over to this instance.
    SecondInstance,
}

/// Motion state machine. Every method takes the current `Instant` so tests drive
/// time explicitly.
#[derive(Debug, Default)]
pub struct HudMotion {
    phase: Phase,
    last_transition: Option<Instant>,
    blur_deadline: Option<Instant>,
}

impl HudMotion {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Global-hotkey / tray-click toggle. Debounced so key repeat and double clicks
    /// cannot race the slide animation.
    pub fn toggle(&mut self, now: Instant) -> Option<Transition> {
        if self.is_debounced(now) {
            return None;
        }
        if self.phase.is_on_screen() {
            self.begin_dismiss(now)
        } else {
            self.begin_summon(now)
        }
    }

    /// Explicit summon: a visible HUD is re-focused but not re-animated, so a second
    /// launch or a tray "显示 HUD" never causes a visible jump.
    pub fn summon(&mut self, now: Instant) -> Option<Transition> {
        if self.phase.is_on_screen() || self.is_debounced(now) {
            return None;
        }
        self.begin_summon(now)
    }

    /// Explicit dismiss (Esc, tray, close request). Debounce is bypassed: pressing Esc
    /// right after a summon must always work.
    pub fn dismiss(&mut self, now: Instant) -> Option<Transition> {
        if !self.phase.is_on_screen() {
            return None;
        }
        self.begin_dismiss(now)
    }

    fn begin_summon(&mut self, now: Instant) -> Option<Transition> {
        self.phase = Phase::Showing;
        self.last_transition = Some(now);
        self.blur_deadline = None;
        Some(Transition::Summon)
    }

    fn begin_dismiss(&mut self, now: Instant) -> Option<Transition> {
        self.phase = Phase::Dismissing;
        self.last_transition = Some(now);
        self.blur_deadline = None;
        Some(Transition::Dismiss)
    }

    fn is_debounced(&self, now: Instant) -> bool {
        self.last_transition
            .map(|last| now.saturating_duration_since(last) < TOGGLE_DEBOUNCE)
            .unwrap_or(false)
    }

    /// The slide-in animation finished.
    pub fn note_shown(&mut self) {
        if self.phase == Phase::Showing {
            self.phase = Phase::Visible;
        }
    }

    /// The slide-out animation finished.
    pub fn note_docked(&mut self) {
        if self.phase.is_dismissing() {
            self.phase = Phase::Hidden;
        }
    }

    /// Window lost focus: start the grace window instead of dismissing immediately.
    pub fn note_focus_lost(&mut self, now: Instant) {
        if self.phase.is_on_screen() && self.blur_deadline.is_none() {
            self.blur_deadline = Some(now + BLUR_GRACE);
        }
    }

    /// Window regained focus inside the grace window: cancel the pending dismiss.
    pub fn note_focus_gained(&mut self) {
        self.blur_deadline = None;
    }

    /// Called by the shell supervisor. Returns a dismiss transition once the grace
    /// window has elapsed without the HUD regaining focus.
    pub fn due_blur_dismiss(&mut self, now: Instant) -> Option<Transition> {
        let deadline = self.blur_deadline?;
        if now < deadline || !self.phase.is_on_screen() {
            return None;
        }
        self.begin_dismiss(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn a_hidden_hud_summons_on_the_first_toggle() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        assert_eq!(motion.phase(), Phase::Hidden);

        assert_eq!(motion.toggle(t0), Some(Transition::Summon));
        assert_eq!(motion.phase(), Phase::Showing);

        motion.note_shown();
        assert_eq!(motion.phase(), Phase::Visible);
    }

    #[test]
    fn toggling_again_inside_the_debounce_window_is_ignored() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.toggle(t0);

        assert_eq!(
            motion.toggle(at(t0, TOGGLE_DEBOUNCE.as_millis() as u64 - 1)),
            None
        );
        assert_eq!(motion.phase(), Phase::Showing);
    }

    #[test]
    fn a_visible_hud_dismisses_on_toggle_and_lands_hidden() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.toggle(t0);
        motion.note_shown();

        assert_eq!(
            motion.toggle(at(t0, TOGGLE_DEBOUNCE.as_millis() as u64 + 1)),
            Some(Transition::Dismiss)
        );
        assert!(motion.phase().is_dismissing());
        motion.note_docked();
        assert_eq!(motion.phase(), Phase::Hidden);
    }

    #[test]
    fn toggling_while_dismissing_summons_again() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.summon(t0);
        motion.note_shown();
        motion.dismiss(at(t0, 500));

        assert_eq!(
            motion.toggle(at(t0, 1000)),
            Some(Transition::Summon),
            "hotkey during the slide-out must bring the HUD back"
        );
        assert_eq!(motion.phase(), Phase::Showing);
    }

    #[test]
    fn explicit_summon_of_a_visible_hud_is_a_focus_noop() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.summon(t0);
        motion.note_shown();

        assert_eq!(motion.summon(at(t0, 1000)), None);
        assert_eq!(motion.phase(), Phase::Visible);
    }

    #[test]
    fn dismiss_of_a_hidden_hud_does_nothing() {
        let mut motion = HudMotion::new();
        assert_eq!(motion.dismiss(Instant::now()), None);
        assert_eq!(motion.phase(), Phase::Hidden);
    }

    #[test]
    fn esc_style_dismiss_is_not_debounced() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.summon(t0);
        motion.note_shown();

        // Same millisecond as the summon finished: an explicit dismiss still wins.
        assert_eq!(motion.dismiss(t0), Some(Transition::Dismiss));
    }

    #[test]
    fn blur_inside_the_grace_window_is_cancelled_by_refocus() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.summon(t0);
        motion.note_shown();

        motion.note_focus_lost(at(t0, 100));
        motion.note_focus_gained();
        assert_eq!(motion.due_blur_dismiss(at(t0, 1000)), None);
        assert_eq!(motion.phase(), Phase::Visible);
    }

    #[test]
    fn blur_after_the_grace_window_dismisses() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.summon(t0);
        motion.note_shown();

        motion.note_focus_lost(at(t0, 100));
        let rough_deadline = 100 + BLUR_GRACE.as_millis() as u64;
        assert_eq!(motion.due_blur_dismiss(at(t0, rough_deadline - 1)), None);
        assert_eq!(
            motion.due_blur_dismiss(at(t0, rough_deadline)),
            Some(Transition::Dismiss)
        );
        assert!(motion.phase().is_dismissing());
    }

    #[test]
    fn blur_while_dismissing_is_ignored() {
        let mut motion = HudMotion::new();
        let t0 = Instant::now();
        motion.summon(t0);
        motion.note_shown();
        motion.dismiss(at(t0, 500));

        motion.note_focus_lost(at(t0, 600));
        assert_eq!(motion.due_blur_dismiss(at(t0, 5000)), None);
        assert!(motion.phase().is_dismissing());
    }

    #[test]
    fn only_explicit_reasons_return_focus_to_the_previous_app() {
        assert!(DismissReason::Escape.restores_previous_app());
        assert!(DismissReason::Hotkey.restores_previous_app());
        assert!(DismissReason::Tray.restores_previous_app());
        assert!(DismissReason::CloseRequested.restores_previous_app());
        assert!(!DismissReason::Blur.restores_previous_app());
    }
}
