//! State management for the Behind the Curtain component.
//!
//! `SliderState` provides a reusable abstraction for slider drag tracking and position clamping.
//! Currently unused in favor of Leptos fine-grained signals, but kept as reference architecture
//! and for potential use in server-side state management or advanced testing scenarios.

use serde::{Deserialize, Serialize};

/// Represents the current state of the slider.
///
/// Provides invariant enforcement: position is always clamped to [0.0, 100.0].
/// Useful for state synchronization, testing, and non-reactive contexts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SliderState {
    position: f32,
    is_dragging: bool,
    animation_speed: f32,
}

#[allow(dead_code)]
impl SliderState {
    pub fn new(position: f32) -> Self {
        Self {
            position: Self::clamp_position(position),
            is_dragging: false,
            animation_speed: 50.0,
        }
    }

    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn set_position(&mut self, position: f32) {
        self.position = Self::clamp_position(position);
    }

    pub fn start_drag(&mut self) {
        self.is_dragging = true;
    }

    pub fn end_drag(&mut self) {
        self.is_dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    fn clamp_position(position: f32) -> f32 {
        position.max(0.0).min(100.0)
    }
}

impl Default for SliderState {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_clamping() {
        assert_eq!(SliderState::new(-10.0).position(), 0.0);
        assert_eq!(SliderState::new(150.0).position(), 100.0);
        assert_eq!(SliderState::new(50.0).position(), 50.0);
    }

    #[test]
    fn test_set_position() {
        let mut state = SliderState::new(0.0);
        state.set_position(75.0);
        assert_eq!(state.position(), 75.0);
        state.set_position(200.0);
        assert_eq!(state.position(), 100.0);
    }

    #[test]
    fn test_dragging() {
        let mut state = SliderState::new(0.0);
        assert!(!state.is_dragging());
        state.start_drag();
        assert!(state.is_dragging());
        state.end_drag();
        assert!(!state.is_dragging());
    }
}
