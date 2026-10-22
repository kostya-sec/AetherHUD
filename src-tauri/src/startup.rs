//! Launch-mode parsing for the resident HUD shell (#26 / #12).
//!
//! The same parser serves the first launch and the second-instance callback, because
//! `--hidden` means "stay in the tray, do not summon" in both of them.

/// How the process was asked to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Manual launch: reveal the HUD once the webview is ready.
    ShowHud,
    /// Autostart / second instance with `--hidden`: stay resident without summoning.
    Resident,
}

/// Command-line flag used by the autostart login item and by repeat launches.
pub const HIDDEN_FLAG: &str = "--hidden";

/// Parses the process arguments. The executable path (argv[0]) is ignored, unknown
/// arguments are ignored as well (v1 reserves `aetherhud <URL>` for a later ticket),
/// and `--hidden` anywhere means Resident.
pub fn parse_launch_mode<I, S>(args: I) -> LaunchMode
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if args
        .into_iter()
        .skip(1)
        .any(|argument| argument.as_ref() == HIDDEN_FLAG)
    {
        LaunchMode::Resident
    } else {
        LaunchMode::ShowHud
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_launch_shows_the_hud() {
        assert_eq!(parse_launch_mode(["aetherhud.exe"]), LaunchMode::ShowHud);
    }

    #[test]
    fn hidden_flag_makes_the_process_resident() {
        assert_eq!(
            parse_launch_mode(["aetherhud.exe", HIDDEN_FLAG]),
            LaunchMode::Resident
        );
    }

    #[test]
    fn hidden_flag_is_detected_anywhere_after_the_executable() {
        assert_eq!(
            parse_launch_mode(["aetherhud.exe", "--unused", HIDDEN_FLAG]),
            LaunchMode::Resident
        );
    }

    #[test]
    fn an_executable_path_alone_never_counts_as_the_flag() {
        // A path that merely contains "--hidden" is not the flag itself.
        assert_eq!(
            parse_launch_mode([r"C:\\program files\\--hidden\\aetherhud.exe"]),
            LaunchMode::ShowHud
        );
    }
}
