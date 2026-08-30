//! D-06: the RAM slider's default, computed from the machine's own
//! physical memory rather than a constant — one `sysinfo` call, next to
//! the other machine facts this crate already resolves for itself
//! (`java::detect_target`, `java::translation_state`).

use sysinfo::System;

/// Total physical memory, in binary gigabytes (bytes / 1024^3).
pub fn total_memory_gb() -> f32 {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.total_memory() as f32 / (1024.0 * 1024.0 * 1024.0)
}

/// D-06's default: half of physical RAM, rounded *down* to the nearest
/// half gigabyte (never recommend more than half of what's actually
/// there — a real host's usable memory is always a little under its
/// advertised size), floored at 3 and capped at 8 to stay inside the
/// slider's own 3..=10 range and never suggest the top quarter of it by
/// default.
pub fn recommended_ram_gb(total_gb: f32) -> f32 {
    let half = total_gb / 2.0;
    let floored_to_half_step = (half * 2.0).floor() / 2.0;
    floored_to_half_step.clamp(3.0, 8.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_4gb_machine_recommends_the_slider_floor() {
        assert_eq!(recommended_ram_gb(4.0), 3.0);
    }

    #[test]
    fn a_32gb_machine_recommends_the_cap() {
        assert_eq!(recommended_ram_gb(32.0), 8.0);
    }

    #[test]
    fn a_15gb_machine_recommends_half_exactly() {
        assert_eq!(recommended_ram_gb(15.0), 7.5);
    }

    #[test]
    fn total_memory_gb_is_never_zero_or_absurd_on_a_real_host() {
        let total = total_memory_gb();
        assert!(total > 0.1, "total_memory_gb() returned {total}, expected a real host's RAM");
        assert!(total < 4096.0, "total_memory_gb() returned {total}, suspiciously large");
    }
}
