// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-21

use crate::display::Size;

fn matches(a: Size, b: Size) -> bool {
    a.width.abs_diff(b.width) <= 2 && a.height.abs_diff(b.height) <= 2
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Resize {
    window: Size,
    view: Size,
}
/// Session state resets on reconnect; unsuccessful window/view pairs survive.
#[derive(Default)]
pub(crate) struct CarlinkResizePolicy {
    enabled: bool,
    window: Option<Size>,
    view: Option<Size>,
    triggered: bool,
    pending: Option<Resize>,
    unhelpful: Vec<Resize>,
}
impl CarlinkResizePolicy {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.pending = None;
        }
    }
    pub fn start_session(&mut self) {
        self.window = None;
        self.view = None;
        self.triggered = false;
    }
    pub fn window_size(&mut self, size: Size) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if self.window.is_none() {
            if let Some(pending) = self.pending.take() {
                if matches(pending.window, size) {
                    self.unhelpful.push(pending);
                }
            }
        }
        self.window = Some(size);
    }
    pub fn view_size(&mut self, size: Size) {
        if size.width > 0 && size.height > 0 {
            self.view = Some(size);
        }
    }
    pub fn reconnect(&mut self, playing: bool) -> bool {
        if !self.enabled || !playing || self.triggered {
            return false;
        }
        let (Some(window), Some(view)) = (self.window, self.view) else {
            return false;
        };
        if matches(window, view)
            || self
                .unhelpful
                .iter()
                .any(|p| matches(p.window, window) && matches(p.view, view))
        {
            return false;
        }
        self.triggered = true;
        self.pending = Some(Resize { window, view });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }
    fn policy() -> CarlinkResizePolicy {
        let mut p = CarlinkResizePolicy::default();
        p.set_enabled(true);
        p
    }
    #[test]
    fn tolerance_and_one_trigger_per_session() {
        for dx in -3..=3i32 {
            for dy in -3..=3i32 {
                let mut p = policy();
                p.window_size(size(1532, 1013));
                p.view_size(size((1532 + dx) as u32, (1013 + dy) as u32));
                assert_eq!(p.reconnect(true), dx.abs() > 2 || dy.abs() > 2);
                assert!(!p.reconnect(true));
            }
        }
    }
    #[test]
    fn unchanged_window_blacklists_pair_but_changed_window_does_not() {
        for helpful in [false, true] {
            let mut p = policy();
            let small = size(1532, 1013);
            let full = size(1814, 1200);
            p.window_size(small);
            p.view_size(full);
            assert!(!p.reconnect(false));
            assert!(p.reconnect(true));
            p.window_size(small);
            p.start_session();
            assert!(!p.reconnect(true));
            p.window_size(if helpful { full } else { size(1534, 1011) });
            p.start_session();
            p.window_size(small);
            p.view_size(full);
            assert_eq!(p.reconnect(true), helpful);
            p.start_session();
            p.window_size(small);
            p.view_size(size(1920, 1080));
            assert!(p.reconnect(true));
        }
    }
    #[test]
    fn missing_sizes_latest_view_and_session_reset() {
        let mut p = CarlinkResizePolicy::default();
        let small = size(1532, 1013);
        let full = size(1814, 1200);
        p.view_size(full);
        assert!(!p.reconnect(true));
        p.window_size(small);
        assert!(!p.reconnect(true));
        p.set_enabled(true);
        assert!(!p.reconnect(false));
        p.view_size(small);
        assert!(!p.reconnect(true));
        p.view_size(full);
        assert!(p.reconnect(true));
        p.set_enabled(false);
        p.set_enabled(true);
        assert!(!p.reconnect(true));
        p.start_session();
        assert!(!p.reconnect(true));
        p.window_size(full);
        assert!(!p.reconnect(true));
        p.view_size(small);
        assert!(p.reconnect(true));
    }
    #[test]
    fn helpful_window_switches_repeat_and_unhelpful_pairs_match_with_tolerance() {
        let small = size(1532, 1013);
        let full = size(1814, 1200);
        let mut p = policy();
        for (window, view) in [(small, full), (full, small), (small, full), (full, small)] {
            p.start_session();
            p.window_size(window);
            p.view_size(window);
            assert!(!p.reconnect(true));
            p.view_size(view);
            assert!(p.reconnect(true));
        }
        let mut p = policy();
        p.window_size(small);
        p.view_size(full);
        assert!(p.reconnect(true));
        for _ in 0..3 {
            p.start_session();
            p.window_size(size(1534, 1011));
            p.view_size(size(1812, 1202));
            assert!(!p.reconnect(true));
        }
        p.view_size(size(1920, 1080));
        assert!(p.reconnect(true));
    }
    #[test]
    fn disabling_clears_pending_and_invalid_sizes_are_ignored() {
        let mut p = policy();
        p.window_size(size(0, 100));
        p.view_size(size(100, 0));
        assert!(!p.reconnect(true));
        p.window_size(size(100, 100));
        p.view_size(size(200, 200));
        assert!(p.reconnect(true));
        p.set_enabled(false);
        p.start_session();
        p.window_size(size(100, 100));
        p.view_size(size(200, 200));
        assert!(!p.reconnect(true));
        p.set_enabled(true);
        assert!(p.reconnect(true));
        p.start_session();
        p.window_size(size(100, 100));
        p.set_enabled(false);
        p.start_session();
        p.set_enabled(true);
        p.window_size(size(100, 100));
        p.view_size(size(200, 200));
        assert!(!p.reconnect(true));
    }
}
