//! Opt-in anti-cloak observability for real-machine verification.
//!
//! Set `VERA20K_CLOAK_DIAG=1` before launching to have the presentation frame
//! and the draw plan report the sensor -> visual character -> material chain
//! that the 2026-11-20 controlled `-SPAWN` runs proved on a real client.
//! Ordinary play never consults the switch beyond one cached bool read and
//! emits no probe lines.

use std::sync::OnceLock;

/// The switch is read once per process: a launch sets it, every pass asks.
pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| flag_enables(std::env::var("VERA20K_CLOAK_DIAG").ok().as_deref()))
}

/// Frame captures are sampled every 300 binary frames; draw plans every 150
/// calls, matching the cadence of the controlled real-machine evidence.
pub(crate) fn sample_frame(binary_frame: u32) -> bool {
    enabled() && binary_frame % 300 == 0
}

pub(crate) fn sample_plan(call: u32) -> bool {
    enabled() && call % 150 == 0
}

fn flag_enables(value: Option<&str>) -> bool {
    value.is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloak_probe_is_strictly_opt_in() {
        assert!(!flag_enables(None));
        assert!(!flag_enables(Some("")));
        assert!(!flag_enables(Some("0")));
        assert!(!flag_enables(Some("false")));
        assert!(!flag_enables(Some("2")));
        assert!(flag_enables(Some("1")));
        assert!(flag_enables(Some("TRUE")));
    }

    #[test]
    fn cloak_probe_samples_nothing_without_the_switch() {
        // Only meaningful when the test process itself did not opt in.
        if std::env::var("VERA20K_CLOAK_DIAG").is_err() {
            assert!(!enabled());
            assert!(!sample_frame(0));
            assert!(!sample_plan(0));
        }
    }
}
