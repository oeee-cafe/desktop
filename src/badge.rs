//! The unread count on the app's icon.
//!
//! The site says how many notifications are unread (chrome.rs sends the
//! number in its toolbar's badge as an `oeee-unread` event whenever it
//! changes), and the app puts it where each system puts one: a number on
//! the launcher's icon on Linux, and on Windows -- whose taskbar has no
//! numbers -- a red dot over the taskbar button.

use tauri::{Runtime, WebviewWindow};

/// The event the site's toolbar sends its unread count in.
pub const EVENT: &str = "oeee-unread";

/// The count in an event's payload: a JSON number, or anything else as none.
pub fn parse(payload: &str) -> i64 {
    payload.trim().parse::<i64>().unwrap_or(0).max(0)
}

pub fn show<R: Runtime>(window: &WebviewWindow<R>, count: i64) {
    #[cfg(not(windows))]
    {
        let _ = window.set_badge_count((count > 0).then_some(count));
    }
    #[cfg(windows)]
    {
        let dot = (count > 0).then(|| tauri::image::Image::new_owned(dot_pixels(), DOT, DOT));
        let _ = window.set_overlay_icon(dot);
    }
}

/// The overlay's size: Windows draws it 16px square over the taskbar button.
#[cfg(any(windows, test))]
const DOT: u32 = 16;

/// A red dot, the system's close-button red, with a white rim so it reads on
/// any icon under it -- in RGBA, antialiased by coverage.
#[cfg(any(windows, test))]
fn dot_pixels() -> Vec<u8> {
    let size = DOT as f32;
    let centre = size / 2.0;
    let mut pixels = Vec::with_capacity((DOT * DOT * 4) as usize);
    for y in 0..DOT {
        for x in 0..DOT {
            let dx = x as f32 + 0.5 - centre;
            let dy = y as f32 + 0.5 - centre;
            let distance = (dx * dx + dy * dy).sqrt();
            // Outer edge at 7.5, the red inside 6.
            let outer = (7.5 - distance + 0.5).clamp(0.0, 1.0);
            let inner = (6.0 - distance + 0.5).clamp(0.0, 1.0);
            let (r, g, b) = (
                (0xc4 as f32 * inner + 255.0 * (1.0 - inner)) as u8,
                (0x2b as f32 * inner + 255.0 * (1.0 - inner)) as u8,
                (0x1c as f32 * inner + 255.0 * (1.0 - inner)) as u8,
            );
            pixels.extend_from_slice(&[r, g, b, (outer * 255.0) as u8]);
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_is_a_number_or_none() {
        assert_eq!(parse("3"), 3);
        assert_eq!(parse(" 12 "), 12);
        assert_eq!(parse("0"), 0);
        assert_eq!(parse("-4"), 0);
        assert_eq!(parse("null"), 0);
        assert_eq!(parse("\"x\""), 0);
    }

    #[test]
    fn the_dot_is_red_inside_and_clear_at_the_corners() {
        let pixels = dot_pixels();
        assert_eq!(pixels.len(), (DOT * DOT * 4) as usize);
        let at = |x: u32, y: u32| {
            let i = ((y * DOT + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        assert_eq!(at(8, 8), [0xc4, 0x2b, 0x1c, 255]);
        assert_eq!(at(0, 0)[3], 0);
    }
}
