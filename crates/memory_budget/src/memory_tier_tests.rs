use super::{MemoryTier, BALANCED_MAX_MIB, LOW_END_MAX_MIB};
use crate::{FixedSystem, SystemFacts};

#[test]
fn a_4_gb_pc_is_low_end_and_a_16_gb_pc_is_high_end() {
    assert_eq!(MemoryTier::from_total_mib(3_900), MemoryTier::LowEnd);
    assert_eq!(MemoryTier::from_total_mib(7_800), MemoryTier::Balanced);
    assert_eq!(MemoryTier::from_total_mib(15_600), MemoryTier::HighEnd);
}

#[test]
fn the_thresholds_belong_to_the_lower_tier() {
    assert_eq!(
        MemoryTier::from_total_mib(LOW_END_MAX_MIB),
        MemoryTier::LowEnd
    );
    assert_eq!(
        MemoryTier::from_total_mib(LOW_END_MAX_MIB + 1),
        MemoryTier::Balanced
    );
    assert_eq!(
        MemoryTier::from_total_mib(BALANCED_MAX_MIB),
        MemoryTier::Balanced
    );
    assert_eq!(
        MemoryTier::from_total_mib(BALANCED_MAX_MIB + 1),
        MemoryTier::HighEnd
    );
}

#[test]
fn unknown_ram_counts_as_low_end_so_the_safe_limits_apply() {
    assert_eq!(MemoryTier::from_total_mib(0), MemoryTier::LowEnd);
}

#[test]
fn detect_reads_the_probe() {
    let probe = FixedSystem(SystemFacts {
        total_mib: 32_000,
        cpu_cores: 16,
    });
    assert_eq!(MemoryTier::detect(&probe), MemoryTier::HighEnd);
}

#[test]
fn tiers_have_stable_names_for_the_settings_screen() {
    assert_eq!(MemoryTier::LowEnd.label(), "Low-end");
    assert_eq!(MemoryTier::Balanced.label(), "Balanced");
    assert_eq!(MemoryTier::HighEnd.label(), "High-end");
}
