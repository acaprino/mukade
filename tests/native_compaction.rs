//! Native OpenCode compaction boundaries (session-interchange spec §4.4).

use mukade::native_compaction::{
    NativeBudget, TokenFactor, estimate_tokens, import_budget, plan_native_compaction,
};
use serde_json::{Value, json};

fn user(id: &str, text: &str) -> Value {
    json!({"id": id, "type": "user", "text": text, "time": {"created": 1}})
}
fn assistant(id: &str, text: &str) -> Value {
    json!({"id": id, "type": "assistant", "content": [{"type": "text", "text": text}], "time": {"created": 2}})
}
fn zero_budget() -> NativeBudget {
    NativeBudget {
        context: 40_000,
        max_output_tokens: Some(32_000),
        text_token_factor: None,
    }
}

#[test]
fn the_budget_stays_below_half_the_window_and_the_native_compaction_trigger() {
    let million = NativeBudget {
        context: 1_000_000,
        max_output_tokens: None,
        text_token_factor: None,
    };
    assert_eq!(import_budget(&million), 500_000); // ceiling 850k - 24k, but half the window wins
    let small = NativeBudget {
        context: 100_000,
        max_output_tokens: Some(0),
        text_token_factor: None,
    };
    assert_eq!(import_budget(&small), 50_000); // ceiling 85k - 8k, half still wins
    assert_eq!(import_budget(&zero_budget()), 0); // never negative: the newest message still survives
}

#[test]
fn token_estimation_uses_chars_three_and_the_gateway_multiplier() {
    assert_eq!(estimate_tokens("a".repeat(90).as_str(), None), 30);
    assert_eq!(
        estimate_tokens(
            "a".repeat(90).as_str(),
            Some(TokenFactor::new(4.0 / 3.0).unwrap())
        ),
        40
    );
    assert_eq!(
        estimate_tokens(
            "a".repeat(90).as_str(),
            Some(TokenFactor::new(0.5).unwrap())
        ),
        30
    ); // factors below one never lower it
}

#[test]
fn a_document_that_fits_needs_no_native_boundary() {
    let messages = vec![user("u1", "hello"), assistant("a1", "world")];
    let budget = NativeBudget {
        context: 1_000_000,
        max_output_tokens: None,
        text_token_factor: None,
    };
    assert!(plan_native_compaction(&messages, &budget).is_none());
}

#[test]
fn over_budget_the_newest_message_is_kept_and_earlier_user_requests_are_summarized() {
    let messages = vec![
        user("u1", "first request"),
        assistant("a1", "OLD-ANSWER-ONE"),
        user("u2", "middle request"),
        assistant("a2", "OLD-ANSWER-TWO"),
        user("u3", "recent request"),
        assistant("a3", "RECENT-ANSWER"),
    ];
    let report = plan_native_compaction(&messages, &zero_budget()).expect("a native boundary");
    assert_eq!(report.keep_id.as_deref(), Some("a3")); // keepID points backwards, to the last pre-boundary message
    assert_eq!(report.start, 5);
    assert_eq!(report.summarized_messages, 5);
    assert_eq!(report.summarized_user_messages, 3);
    assert!(report.summary.contains("first request"));
    assert!(
        report.summary.find("first request").unwrap()
            < report.summary.find("middle request").unwrap()
    );
    assert!(report.summary.contains("recent request"));
    assert!(!report.summary.contains("OLD-ANSWER-ONE"));
    assert!(!report.summary.contains("OLD-ANSWER-TWO"));
    assert!(report.projected_tokens < report.estimated_tokens);
    assert!(!report.fits); // even the retained newest message is over a zero budget: report, never lie
}

#[test]
fn each_user_request_is_truncated_in_the_deterministic_summary() {
    let long = "x".repeat(5_000);
    let messages = vec![user("u1", &long), assistant("a2", "latest")];
    let report = plan_native_compaction(&messages, &zero_budget()).unwrap();
    assert!(report.summary.contains('…'));
    assert!(report.summary.len() < 2_000);
    assert!(!report.summary.contains(&long));
}
