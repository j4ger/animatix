//! CLI entry-point acceptance. Semantic samples and fault rejection are shared
//! by the engine and SDK tests, not inferred from this command's exit status.
use std::process::Command;
mod common;

#[test]
fn sorting_fixture_checks_and_audits_without_gpu() {
    let (_project, input) = common::temp_project(
        "sorting-contract",
        include_str!("../../animatix/tests/contracts/sorting.amx"),
    );
    let check = Command::new(env!("CARGO_BIN_EXE_animatix"))
        .arg("check")
        .arg(&input)
        .output()
        .expect("CLI check");
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));
    let audit = Command::new(env!("CARGO_BIN_EXE_animatix"))
        .arg("audit")
        .arg(&input)
        .args([
            "--format",
            "json",
            "--render",
            "never",
            "--fail-on",
            "warning",
        ])
        .output()
        .expect("CLI audit");
    assert!(
        audit.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&audit.stdout),
        String::from_utf8_lossy(&audit.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&audit.stdout).expect("audit JSON");
    assert_eq!(json["summary"]["errors"], 0);
    assert_eq!(json["summary"]["warnings"], 0);
    assert_eq!(json["render"]["status"], "skipped");
}
