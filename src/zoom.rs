//! Making the whole window bigger or smaller: Ctrl+Plus, Ctrl+Minus and
//! Ctrl+0 (keys.rs), as in any Windows browser. WebView2 has these itself,
//! but they are among the browser keys the app turns off (webview2.rs), and
//! its own would forget the size between launches, which is when someone who
//! needs bigger type most wants it kept.
//!
//! Not the pinch or Ctrl+wheel, which the site turns away in the app on
//! purpose (theme_head.jinja in oeee-cafe/web): a trackpad pinch over the
//! painter is a canvas zoom asked for, not a page one.

use std::sync::Mutex;

use tauri::{Manager, WebviewWindow};

/// Which way a key moves the size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    In,
    Out,
    Reset,
}

/// The sizes a key moves between: Edge's, from half to three times, so each
/// press is the step a Windows reader's hands expect.
const LEVELS: [f64; 13] = [
    0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];

/// The size one press of `step` moves `current` to.
pub fn next(current: f64, step: Step) -> f64 {
    const CLOSE: f64 = 0.005;
    match step {
        Step::Reset => 1.0,
        Step::In => LEVELS
            .iter()
            .copied()
            .find(|level| *level > current + CLOSE)
            .unwrap_or(LEVELS[LEVELS.len() - 1]),
        Step::Out => LEVELS
            .iter()
            .rev()
            .copied()
            .find(|level| *level < current - CLOSE)
            .unwrap_or(LEVELS[0]),
    }
}

/// The size now, which WebView2 keeps from page to page on its own.
static ZOOM: Mutex<f64> = Mutex::new(1.0);

const FILE: &str = "zoom";

/// Puts the window at the size it was left at.
pub fn restore(window: &WebviewWindow) {
    let Some(saved) = file(window)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|level| (LEVELS[0]..=LEVELS[LEVELS.len() - 1]).contains(level))
    else {
        return;
    };
    *ZOOM.lock().unwrap() = saved;
    let _ = window.set_zoom(saved);
}

/// Moves the size one step, and keeps it for the next launch.
pub fn change(window: &WebviewWindow, step: Step) {
    let level = {
        let mut zoom = ZOOM.lock().unwrap();
        *zoom = next(*zoom, step);
        *zoom
    };
    let _ = window.set_zoom(level);
    if let Some(path) = file(window) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, level.to_string());
    }
}

fn file(window: &WebviewWindow) -> Option<std::path::PathBuf> {
    window
        .path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_moves_one_level() {
        assert_eq!(next(1.0, Step::In), 1.1);
        assert_eq!(next(1.0, Step::Out), 0.9);
        assert_eq!(next(1.75, Step::Reset), 1.0);
    }

    #[test]
    fn the_ends_hold() {
        assert_eq!(next(3.0, Step::In), 3.0);
        assert_eq!(next(0.5, Step::Out), 0.5);
    }

    #[test]
    fn a_size_between_levels_goes_to_the_next_one_along() {
        assert_eq!(next(1.2, Step::In), 1.25);
        assert_eq!(next(1.2, Step::Out), 1.1);
    }
}
