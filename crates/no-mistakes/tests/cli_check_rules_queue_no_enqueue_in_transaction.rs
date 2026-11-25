use super::check_json_snapshot::{assert_saved_fixture_report, rules_path};

const RULE: &str = "queue-no-enqueue-in-transaction";

#[test]
fn queue_no_enqueue_in_transaction_json_matches_snapshot() {
    let root = rules_path(RULE, "fixture", "behavior");
    let incomplete = rules_path(RULE, "fixture", "incomplete");
    let reports = assert_saved_fixture_report(
        "queue_no_enqueue_in_transaction",
        &[
            ("configured", root.clone(), root.join(".no-mistakes.yml")),
            ("default_off", root.clone(), root.join("default.yml")),
            (
                "incomplete",
                incomplete.clone(),
                incomplete.join(".no-mistakes.yml"),
            ),
        ],
        &[],
        &["--include-suppressed"],
    );

    let configured = &reports["configured"];
    assert_eq!(configured["exitCode"], 1);
    let findings = configured["report"]["rules"].as_array().unwrap();
    let mut locations: Vec<_> = findings
        .iter()
        .map(|finding| {
            assert_eq!(finding["rule"], RULE);
            (
                finding["file"].as_str().unwrap(),
                finding["line"].as_u64().unwrap(),
            )
        })
        .collect();
    locations.sort_unstable();
    assert_eq!(
        locations,
        [("src/flows/receipt.ts", 6), ("src/flows/receipt.ts", 9)]
    );
    assert_eq!(
        configured["report"]["suppressed"].as_array().unwrap().len(),
        3
    );

    let default = &reports["default_off"];
    assert_eq!(default["exitCode"], 0);
    assert_eq!(default["report"]["rules"], serde_json::json!([]));
    let incomplete = &reports["incomplete"]["report"];
    let failures = incomplete["incomplete"].as_array().unwrap();
    assert!(failures
        .iter()
        .any(|failure| { failure["consumer"] == RULE && failure["file"] == "src/broken.ts" }));
    let healthy = incomplete["rules"].as_array().unwrap();
    assert_eq!(healthy.len(), 1);
    assert_eq!(healthy[0]["file"], "src/healthy.ts");
    assert!(healthy[0]["message"]
        .as_str()
        .unwrap()
        .starts_with("Queue handoff"));
}
