//! Animation and transition logic for the Behind the Curtain component.
//!
//! Pure functions for computing opacity transitions and offsets.
//! All functions are fully tested and available for reuse in future features.

pub fn art_opacity(position: f32) -> f32 {
    let clamped = position.max(0.0).min(100.0);
    (100.0 - clamped) / 100.0
}

pub fn technique_opacity(position: f32) -> f32 {
    let clamped = position.max(0.0).min(100.0);
    clamped / 100.0
}

/// Calculates the pixel offset of the slider thumb based on position and container width.
///
/// Currently unused in the reactive component (CSS transforms handle it),
/// but kept for future server-side positioning logic and comprehensive test coverage.
#[allow(dead_code)]
pub fn slider_offset(position: f32, container_width: f32) -> f32 {
    let clamped = position.max(0.0).min(100.0);
    (clamped / 100.0) * container_width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_art_opacity_boundaries() {
        assert_eq!(art_opacity(0.0), 1.0);
        assert_eq!(art_opacity(100.0), 0.0);
    }

    #[test]
    fn test_art_opacity_midpoint() {
        assert!((art_opacity(50.0) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_technique_opacity_boundaries() {
        assert_eq!(technique_opacity(0.0), 0.0);
        assert_eq!(technique_opacity(100.0), 1.0);
    }

    #[test]
    fn test_technique_opacity_midpoint() {
        assert!((technique_opacity(50.0) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_opacities_sum() {
        for pos in (0..=100).step_by(10) {
            let pos_f = pos as f32;
            let sum = art_opacity(pos_f) + technique_opacity(pos_f);
            assert!((sum - 1.0).abs() < 0.01, "Opacities should sum to ~1.0");
        }
    }

    #[test]
    fn test_slider_offset() {
        assert_eq!(slider_offset(0.0, 1000.0), 0.0);
        assert_eq!(slider_offset(100.0, 1000.0), 1000.0);
        assert_eq!(slider_offset(50.0, 1000.0), 500.0);
    }
}
