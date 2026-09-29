use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationCase {
    id: String,
    language: String,
    category: String,
    question: String,
    expected_route: String,
    expected_evidence: Vec<String>,
    quality_checks: Vec<String>,
}

fn cases() -> Vec<EvaluationCase> {
    serde_json::from_str(include_str!("../../tests/ai-prompt-evaluation.json"))
        .expect("prompt evaluation JSON must remain valid")
}

#[test]
fn evaluation_set_covers_required_languages_and_question_families() {
    let cases = cases();
    assert!(cases.len() >= 10);
    let languages = cases
        .iter()
        .map(|case| case.language.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(languages, HashSet::from(["zh", "en", "mixed"]));
    let categories = cases
        .iter()
        .map(|case| case.category.as_str())
        .collect::<HashSet<_>>();
    for required in [
        "person_name",
        "risk_analysis",
        "portfolio_summary",
        "comparison",
        "milestone",
        "followup",
        "insufficient_data",
    ] {
        assert!(
            categories.contains(required),
            "missing evaluation category: {required}"
        );
    }
}

#[test]
fn every_evaluation_case_has_a_bounded_expected_contract() {
    let allowed_routes = [
        "local_sqlite",
        "controlled_plan",
        "current_chat",
        "clarification_or_insufficient",
    ];
    for case in cases() {
        assert!(!case.id.trim().is_empty());
        assert!(!case.question.trim().is_empty());
        assert!(allowed_routes.contains(&case.expected_route.as_str()));
        assert!(!case.expected_evidence.is_empty());
        assert!(case.quality_checks.len() >= 2);
    }
}
