use std::collections::{BTreeMap, HashSet};

use crate::model::{
    DxDiagnostic, DxFinding, DxRuleCategoryDefinition, DxScoreBucketSummary, DxScoreStatus,
    DxScoreSummary, DxSeverity,
};

const DEFAULT_MAX_SCORE: u16 = 500;

pub fn summarize_score(findings: &[DxFinding], diagnostics: &[DxDiagnostic]) -> DxScoreSummary {
    summarize_score_with_categories(findings, diagnostics, &[])
}

pub fn summarize_score_with_categories(
    findings: &[DxFinding],
    diagnostics: &[DxDiagnostic],
    categories: &[DxRuleCategoryDefinition],
) -> DxScoreSummary {
    let finding_weight_total = findings
        .iter()
        .map(|finding| finding.weight)
        .fold(0u16, u16::saturating_add);
    let max_score = DEFAULT_MAX_SCORE;
    let score = max_score.saturating_sub(finding_weight_total.min(max_score));

    let real_diagnostics = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.source != "dx-check-rule")
        .collect::<Vec<_>>();
    let failure_count = findings
        .iter()
        .filter(|finding| finding.severity == DxSeverity::Failure)
        .count()
        + real_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DxSeverity::Failure)
            .count();
    let warning_count = findings
        .iter()
        .filter(|finding| finding.severity == DxSeverity::Warning)
        .count()
        + real_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DxSeverity::Warning)
            .count();
    let info_count = findings
        .iter()
        .filter(|finding| finding.severity == DxSeverity::Info)
        .count()
        + real_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == DxSeverity::Info)
            .count();

    let score_percent = percent(score, max_score);
    let status = if failure_count > 0 || score_percent < 70 {
        DxScoreStatus::Blocked
    } else if warning_count > 0 || score_percent < 90 {
        DxScoreStatus::Warning
    } else {
        DxScoreStatus::Ready
    };

    DxScoreSummary {
        score,
        max_score,
        status,
        finding_weight_total,
        failure_count,
        warning_count,
        info_count,
        buckets: summarize_buckets(findings, categories),
        ..DxScoreSummary::default()
    }
}

fn summarize_buckets(
    findings: &[DxFinding],
    categories: &[DxRuleCategoryDefinition],
) -> Vec<DxScoreBucketSummary> {
    if categories.is_empty() && findings.is_empty() {
        return Vec::new();
    }

    let mut definitions = Vec::<DxRuleCategoryDefinition>::new();
    let mut defined_ids = HashSet::<String>::new();
    for category in categories {
        if defined_ids.insert(category.id.clone()) {
            definitions.push(category.clone());
        }
    }

    let mut unknown_finding_weight = BTreeMap::<String, u16>::new();
    for finding in findings {
        if !defined_ids.contains(&finding.category) {
            unknown_finding_weight
                .entry(finding.category.clone())
                .and_modify(|weight| *weight = weight.saturating_add(finding.weight))
                .or_insert(finding.weight);
        }
    }
    for (id, weight) in unknown_finding_weight {
        definitions.push(DxRuleCategoryDefinition {
            label: id.clone(),
            id,
            weight: weight.clamp(1, 100),
        });
    }

    definitions
        .iter()
        .map(|category| summarize_bucket(category, findings))
        .collect()
}

fn summarize_bucket(
    category: &DxRuleCategoryDefinition,
    findings: &[DxFinding],
) -> DxScoreBucketSummary {
    let bucket_findings = findings
        .iter()
        .filter(|finding| finding.category == category.id)
        .collect::<Vec<_>>();
    let finding_weight_total = bucket_findings
        .iter()
        .map(|finding| finding.weight)
        .fold(0u16, u16::saturating_add);
    let max_score = category.weight;
    let score = max_score.saturating_sub(finding_weight_total.min(max_score));
    let failure_count = bucket_findings
        .iter()
        .filter(|finding| finding.severity == DxSeverity::Failure)
        .count();
    let warning_count = bucket_findings
        .iter()
        .filter(|finding| finding.severity == DxSeverity::Warning)
        .count();
    let info_count = bucket_findings
        .iter()
        .filter(|finding| finding.severity == DxSeverity::Info)
        .count();
    let status = bucket_status(max_score, score, failure_count, warning_count);

    DxScoreBucketSummary {
        id: category.id.clone(),
        label: category.label.clone(),
        score,
        max_score,
        status,
        finding_weight_total,
        failure_count,
        warning_count,
        info_count,
    }
}

fn bucket_status(
    max_score: u16,
    score: u16,
    failure_count: usize,
    warning_count: usize,
) -> DxScoreStatus {
    if failure_count > 0 {
        return DxScoreStatus::Blocked;
    }
    if max_score == 0 {
        return if warning_count > 0 {
            DxScoreStatus::Warning
        } else {
            DxScoreStatus::Ready
        };
    }
    let percent = percent(score, max_score);
    if percent < 70 {
        DxScoreStatus::Blocked
    } else if warning_count > 0 || percent < 90 {
        DxScoreStatus::Warning
    } else {
        DxScoreStatus::Ready
    }
}

fn percent(score: u16, max_score: u16) -> u16 {
    if max_score == 0 {
        return 100;
    }
    ((score as u32 * 100) / max_score as u32).min(100) as u16
}

#[cfg(test)]
mod tests {
    use crate::model::{
        DxDiagnostic, DxFinding, DxMeasurementKind, DxRuleCategoryDefinition, DxScoreStatus,
        DxSeverity,
    };
    use crate::scoring::{summarize_score, summarize_score_with_categories};

    #[test]
    fn weighted_warning_and_failure_findings_reduce_engine_score() {
        let findings = vec![
            finding("line-budget", DxSeverity::Warning, 8),
            finding("generated-source", DxSeverity::Failure, 20),
            finding("orientation", DxSeverity::Info, 5),
        ];

        let score = summarize_score(&findings, &[]);

        assert_eq!(score.schema_version, "dx.check.engine_score.v1");
        assert_eq!(score.profile, "dx-check-engine.rules.v1");
        assert_eq!(score.score, 467);
        assert_eq!(score.max_score, 500);
        assert_eq!(score.finding_weight_total, 33);
        assert_eq!(score.failure_count, 1);
        assert_eq!(score.warning_count, 1);
        assert_eq!(score.info_count, 1);
        assert_eq!(score.status, DxScoreStatus::Blocked);
    }

    #[test]
    fn engine_score_clamps_at_zero() {
        let findings = vec![
            finding("large-file", DxSeverity::Warning, 300),
            finding("secret", DxSeverity::Failure, 300),
        ];

        let score = summarize_score(&findings, &[]);

        assert_eq!(score.score, 0);
        assert_eq!(score.finding_weight_total, 600);
        assert_eq!(score.status, DxScoreStatus::Blocked);
    }

    #[test]
    fn failure_diagnostics_block_even_without_finding_weight() {
        let diagnostics = vec![diagnostic("json-syntax-error", DxSeverity::Failure)];

        let score = summarize_score(&[], &diagnostics);

        assert_eq!(score.score, 500);
        assert_eq!(score.finding_weight_total, 0);
        assert_eq!(score.failure_count, 1);
        assert_eq!(score.status, DxScoreStatus::Blocked);
    }

    #[test]
    fn mirrored_rule_diagnostics_do_not_double_count_findings() {
        let findings = vec![finding("line-budget", DxSeverity::Warning, 8)];
        let diagnostics = vec![diagnostic_with_source(
            "line-budget",
            "dx-check-rule",
            DxSeverity::Warning,
        )];

        let score = summarize_score(&findings, &diagnostics);

        assert_eq!(score.score, 492);
        assert_eq!(score.finding_weight_total, 8);
        assert_eq!(score.warning_count, 1);
        assert_eq!(score.status, DxScoreStatus::Warning);
    }

    #[test]
    fn category_buckets_score_against_declared_weights() {
        let findings = vec![
            finding("component-budget", DxSeverity::Warning, 23),
            DxFinding {
                category: "test-readiness".to_string(),
                ..finding("missing-tests", DxSeverity::Warning, 10)
            },
        ];
        let categories = vec![
            category("structure", "Structure", 70),
            category("test-readiness", "Test readiness", 30),
        ];

        let score = summarize_score_with_categories(&findings, &[], &categories);

        assert_eq!(score.score, 467);
        assert_eq!(score.max_score, 500);
        assert_eq!(score.finding_weight_total, 33);

        let structure = score
            .buckets
            .iter()
            .find(|bucket| bucket.id == "structure")
            .unwrap();
        assert_eq!(structure.score, 47);
        assert_eq!(structure.max_score, 70);
        assert_eq!(structure.status, DxScoreStatus::Blocked);

        let tests = score
            .buckets
            .iter()
            .find(|bucket| bucket.id == "test-readiness")
            .unwrap();
        assert_eq!(tests.score, 20);
        assert_eq!(tests.max_score, 30);
        assert_eq!(tests.status, DxScoreStatus::Blocked);
    }

    #[test]
    fn engine_score_stays_on_five_hundred_point_contract_with_heavy_categories() {
        let findings = vec![finding("huge-weight", DxSeverity::Failure, u16::MAX)];
        let categories = vec![
            category("structure", "Structure", u16::MAX),
            category("test-readiness", "Test readiness", u16::MAX),
        ];

        let score = summarize_score_with_categories(&findings, &[], &categories);

        assert_eq!(score.score, 0);
        assert_eq!(score.max_score, 500);
        assert_eq!(score.finding_weight_total, u16::MAX);
        assert_eq!(score.status, DxScoreStatus::Blocked);
    }

    #[test]
    fn duplicate_category_definitions_keep_first_loaded_bucket_contract() {
        let findings = vec![finding("component-budget", DxSeverity::Warning, 23)];
        let categories = vec![
            category("structure", "Trusted Structure", 70),
            category("structure", "Shadow Structure", 1),
        ];

        let score = summarize_score_with_categories(&findings, &[], &categories);
        let structure = score
            .buckets
            .iter()
            .find(|bucket| bucket.id == "structure")
            .unwrap();

        assert_eq!(structure.label, "Trusted Structure");
        assert_eq!(structure.max_score, 70);
        assert_eq!(structure.score, 47);
    }

    fn finding(id: &str, severity: DxSeverity, weight: u16) -> DxFinding {
        DxFinding {
            id: id.to_string(),
            category: "structure".to_string(),
            severity,
            message: "finding".to_string(),
            next_action: "fix it".to_string(),
            measurement: DxMeasurementKind::Measured,
            file: None,
            actual: None,
            threshold: None,
            weight,
            docs: None,
            provenance: None,
        }
    }

    fn diagnostic(id: &str, severity: DxSeverity) -> DxDiagnostic {
        diagnostic_with_source(id, "syntax", severity)
    }

    fn diagnostic_with_source(id: &str, source: &str, severity: DxSeverity) -> DxDiagnostic {
        DxDiagnostic {
            id: id.to_string(),
            source: source.to_string(),
            severity,
            file: None,
            line: None,
            column: None,
            message: "diagnostic".to_string(),
            next_action: "fix it".to_string(),
            measurement: DxMeasurementKind::Measured,
        }
    }

    fn category(id: &str, label: &str, weight: u16) -> DxRuleCategoryDefinition {
        DxRuleCategoryDefinition {
            id: id.to_string(),
            label: label.to_string(),
            weight,
        }
    }
}
