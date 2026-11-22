use super::{FixedSystem, SystemFacts, SystemProbe};

#[test]
fn fixed_system_reports_exactly_the_facts_it_was_given() {
    let facts = SystemFacts {
        total_mib: 3_900,
        cpu_cores: 4,
    };
    assert_eq!(FixedSystem(facts).facts(), facts);
}
