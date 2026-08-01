use std::path::Path;

use pidra::process::{ProcessState, procfs::scan_procfs};

#[test]
fn scans_fixture_procfs_with_partial_data() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/proc");

    let snapshots = scan_procfs(&root).expect("fixture root is readable");

    assert_eq!(
        snapshots.len(),
        2,
        "invalid and vanished entries are skipped"
    );
    let normal = snapshots
        .iter()
        .find(|snapshot| snapshot.identity.pid == 100)
        .expect("normal fixture");
    assert_eq!(normal.name, "normal app");
    assert_eq!(normal.identity.start_time_ticks, 12_345);
    assert_eq!(normal.state, ProcessState::Sleeping);
    assert_eq!(normal.uid, 1_000);
    assert_eq!(normal.thread_count, 4);
    assert_eq!(normal.read_bytes, Some(4_096));
    assert_eq!(normal.write_bytes, Some(8_192));
    assert_eq!(normal.cgroups.len(), 1);
    assert_eq!(
        normal.rss_bytes,
        10 * u64::try_from(rustix::param::page_size()).expect("page size fits u64")
    );

    let unusual = snapshots
        .iter()
        .find(|snapshot| snapshot.identity.pid == 101)
        .expect("unusual comm fixture");
    assert_eq!(unusual.name, "weird ) name (ok)");
    assert_eq!(unusual.parent_pid, Some(100));
    assert_eq!(unusual.state, ProcessState::Running);
    assert!(unusual.cgroups.is_empty());
}

#[test]
fn missing_procfs_root_is_a_typed_error() {
    let error = scan_procfs(Path::new("tests/fixtures/does-not-exist"))
        .expect_err("missing root must fail");

    assert!(error.to_string().contains("cannot read procfs root"));
}
