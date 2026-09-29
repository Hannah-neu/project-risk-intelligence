use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, Result};

pub(crate) const RISK_RULE_VERSION: &str = "1";
pub(crate) const MAX_RISK_SCORE: i64 = 100;
pub(crate) const MEDIUM_RISK_MIN: i64 = 25;
pub(crate) const HIGH_RISK_MIN: i64 = 50;
pub(crate) const CRITICAL_RISK_MIN: i64 = 75;
pub(crate) const OVERDUE_POINTS: i64 = 20;
pub(crate) const MULTIPLE_OVERDUE_POINTS: i64 = 10;
pub(crate) const HOLD_POINTS: i64 = 15;
pub(crate) const COMPLETED_LATE_POINTS: [(i64, i64); 3] = [(8, 10), (15, 20), (31, 30)];

pub(crate) fn methodology_context() -> serde_json::Value {
    serde_json::json!({
        "ruleVersion": RISK_RULE_VERSION,
        "score": {"startsAt": 0, "maximum": MAX_RISK_SCORE},
        "overdue": {"condition": "planned_date < local today AND actual_date IS NULL", "pointsPerMilestone": OVERDUE_POINTS},
        "completedLate": COMPLETED_LATE_POINTS.iter().map(|(minimum_days, points)|
            serde_json::json!({"minimumDelayDays": minimum_days, "points": points})).collect::<Vec<_>>(),
        "multipleOverdue": {"minimumMilestones": 2, "additionalPoints": MULTIPLE_OVERDUE_POINTS},
        "holdStatus": {"value": "HOLD", "additionalPoints": HOLD_POINTS},
        "riskLevels": {"Low": [0, MEDIUM_RISK_MIN-1], "Medium": [MEDIUM_RISK_MIN, HIGH_RISK_MIN-1],
            "High": [HIGH_RISK_MIN, CRITICAL_RISK_MIN-1], "Critical": [CRITICAL_RISK_MIN, MAX_RISK_SCORE]},
        "futurePlannedMilestoneAddsRisk": false
    })
}
const LAST_RISK_CALCULATION_DATE_KEY: &str = "last_risk_calculated_date";
const RISK_RULE_VERSION_KEY: &str = "risk_rule_version";

#[derive(Debug)]
struct MilestoneState {
    milestone_type: String,
    planned_date: Option<String>,
    actual_date: Option<String>,
}

#[derive(Debug)]
struct RiskEvaluation {
    score: i64,
    level: &'static str,
    primary_driver: Option<String>,
    reasons: Vec<String>,
}

#[derive(Debug)]
struct ProjectRiskInput {
    id: i64,
    project_status: Option<String>,
    milestones: Vec<MilestoneState>,
}

#[derive(Debug, PartialEq)]
enum MilestoneStatus {
    NotPlanned,
    NotCompleted,
    Overdue,
    CompletedOnTime,
    CompletedLate,
}

impl MilestoneStatus {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::NotPlanned => "Not planned",
            Self::NotCompleted => "Not completed",
            Self::Overdue => "Overdue",
            Self::CompletedOnTime => "Completed on time",
            Self::CompletedLate => "Completed late",
        }
    }
}

#[derive(Debug)]
struct MilestoneAnalysis {
    status: MilestoneStatus,
    score: i64,
    driver_rank: i64,
    reason: Option<String>,
}

pub fn recalculate_all(connection: &Connection) -> Result<()> {
    let projects = {
        let mut statement = connection.prepare(
            r#"
            SELECT p.id, p.project_status,
              m.milestone_type, m.planned_date, m.actual_date
            FROM projects p
            LEFT JOIN project_milestones m ON m.project_id = p.id
            ORDER BY p.id, m.id
            "#,
        )?;
        let mut rows = statement.query([])?;
        let mut projects: Vec<ProjectRiskInput> = Vec::new();

        while let Some(row) = rows.next()? {
            let project_id = row.get::<_, i64>(0)?;
            if projects
                .last()
                .is_none_or(|project| project.id != project_id)
            {
                projects.push(ProjectRiskInput {
                    id: project_id,
                    project_status: row.get(1)?,
                    milestones: Vec::new(),
                });
            }

            if let Some(milestone_type) = row.get::<_, Option<String>>(2)? {
                projects
                    .last_mut()
                    .expect("the current project was inserted above")
                    .milestones
                    .push(MilestoneState {
                        milestone_type,
                        planned_date: row.get(3)?,
                        actual_date: row.get(4)?,
                    });
            }
        }

        projects
    };

    connection.execute("DELETE FROM project_risks", [])?;
    let mut insert_statement = connection.prepare(
        r#"
        INSERT INTO project_risks (
          project_id, risk_score, risk_level, primary_risk_driver, risk_reasons
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )?;

    for project in projects {
        let risk = evaluate(project.project_status.as_deref(), &project.milestones);
        insert_statement.execute(params![
            project.id,
            risk.score,
            risk.level,
            risk.primary_driver,
            serde_json::to_string(&risk.reasons).unwrap_or_else(|_| "[]".to_string()),
        ])?;
    }

    Ok(())
}

pub(crate) fn cache_is_current(connection: &Connection, today: NaiveDate) -> Result<bool> {
    let calculated_date =
        crate::repositories::settings::get(connection, LAST_RISK_CALCULATION_DATE_KEY)?;
    let rule_version = crate::repositories::settings::get(connection, RISK_RULE_VERSION_KEY)?;
    let expected_date = today.format("%Y-%m-%d").to_string();

    if calculated_date.as_deref() != Some(expected_date.as_str())
        || rule_version.as_deref() != Some(RISK_RULE_VERSION)
    {
        return Ok(false);
    }

    let (project_count, risk_count) = connection.query_row(
        "SELECT (SELECT COUNT(*) FROM projects), (SELECT COUNT(*) FROM project_risks)",
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let insight_count = connection.query_row("SELECT COUNT(*) FROM ai_insights", [], |row| {
        row.get::<_, i64>(0)
    })?;

    Ok(project_count == risk_count && insight_count > 0)
}

pub(crate) fn mark_cache_current(connection: &Connection, today: NaiveDate) -> Result<()> {
    crate::repositories::settings::upsert(
        connection,
        LAST_RISK_CALCULATION_DATE_KEY,
        &today.format("%Y-%m-%d").to_string(),
    )?;
    crate::repositories::settings::upsert(connection, RISK_RULE_VERSION_KEY, RISK_RULE_VERSION)?;
    Ok(())
}

fn evaluate(project_status: Option<&str>, milestones: &[MilestoneState]) -> RiskEvaluation {
    evaluate_at(project_status, milestones, Local::now().date_naive())
}

fn evaluate_at(
    project_status: Option<&str>,
    milestones: &[MilestoneState],
    today: NaiveDate,
) -> RiskEvaluation {
    let mut score = 0_i64;
    let mut overdue_count = 0_i64;
    let mut reasons = Vec::new();
    let mut ranked_drivers: Vec<(i64, String)> = Vec::new();

    for milestone in milestones {
        let analysis = analyze_milestone(milestone, today);
        score += analysis.score;
        overdue_count += i64::from(analysis.status == MilestoneStatus::Overdue);
        if let Some(reason) = analysis.reason {
            reasons.push(reason.clone());
            ranked_drivers.push((analysis.driver_rank, reason));
        }
    }

    if overdue_count >= 2 {
        score += MULTIPLE_OVERDUE_POINTS;
        let reason = format!("{overdue_count} milestones are overdue");
        reasons.push(reason.clone());
        ranked_drivers.push((25, reason));
    }

    if project_status.is_some_and(|status| status.trim().eq_ignore_ascii_case("HOLD")) {
        score += HOLD_POINTS;
        let reason = "Project status is HOLD".to_string();
        reasons.push(reason.clone());
        ranked_drivers.push((15, reason));
    }

    let score = score.min(MAX_RISK_SCORE);
    let level = risk_level(score);
    ranked_drivers.sort_by_key(|driver| std::cmp::Reverse(driver.0));

    RiskEvaluation {
        score,
        level,
        primary_driver: ranked_drivers.into_iter().next().map(|(_, reason)| reason),
        reasons,
    }
}

fn analyze_milestone(milestone: &MilestoneState, today: NaiveDate) -> MilestoneAnalysis {
    let planned = parse_date(milestone.planned_date.as_deref());
    let actual = parse_date(milestone.actual_date.as_deref());

    match (planned, actual) {
        (None, None) => milestone_analysis(MilestoneStatus::NotPlanned, 0, 0, None),
        (None, Some(_)) => milestone_analysis(MilestoneStatus::CompletedOnTime, 0, 0, None),
        (Some(planned), Some(actual)) => {
            let delay_days = (actual - planned).num_days();
            let score = completed_delay_score(delay_days);
            if delay_days > 0 {
                let reason = (score > 0).then(|| {
                    format!(
                        "{} completed {} days late",
                        milestone.milestone_type, delay_days
                    )
                });
                milestone_analysis(MilestoneStatus::CompletedLate, score, score, reason)
            } else {
                milestone_analysis(MilestoneStatus::CompletedOnTime, 0, 0, None)
            }
        }
        (Some(planned), None) if planned < today => {
            let overdue_days = (today - planned).num_days();
            let reason = format!(
                "{} overdue by {} day{}",
                milestone.milestone_type,
                overdue_days,
                if overdue_days == 1 { "" } else { "s" }
            );
            milestone_analysis(
                MilestoneStatus::Overdue,
                OVERDUE_POINTS,
                OVERDUE_POINTS + overdue_days.min(30),
                Some(reason),
            )
        }
        (Some(_), None) => milestone_analysis(MilestoneStatus::NotCompleted, 0, 0, None),
    }
}

pub(crate) fn milestone_display(
    milestone_type: &str,
    planned_date: Option<&str>,
    actual_date: Option<&str>,
    today: NaiveDate,
) -> (&'static str, Option<i64>) {
    let milestone = MilestoneState {
        milestone_type: milestone_type.to_string(),
        planned_date: planned_date.map(str::to_string),
        actual_date: actual_date.map(str::to_string),
    };
    let analysis = analyze_milestone(&milestone, today);
    let planned = parse_date(planned_date);
    let actual = parse_date(actual_date);
    let delta_days = match (planned, actual) {
        (Some(planned), Some(actual)) => Some((actual - planned).num_days()),
        (Some(planned), None) if planned < today => Some((today - planned).num_days()),
        _ => None,
    };
    (analysis.status.label(), delta_days)
}

fn milestone_analysis(
    status: MilestoneStatus,
    score: i64,
    driver_rank: i64,
    reason: Option<String>,
) -> MilestoneAnalysis {
    MilestoneAnalysis {
        status,
        score,
        driver_rank,
        reason,
    }
}

fn parse_date(value: Option<&str>) -> Option<NaiveDate> {
    value.and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
}

fn completed_delay_score(delay_days: i64) -> i64 {
    COMPLETED_LATE_POINTS
        .iter()
        .rev()
        .find(|(minimum_days, _)| delay_days >= *minimum_days)
        .map_or(0, |(_, points)| *points)
}

fn risk_level(score: i64) -> &'static str {
    if score >= CRITICAL_RISK_MIN {
        "Critical"
    } else if score >= HIGH_RISK_MIN {
        "High"
    } else if score >= MEDIUM_RISK_MIN {
        "Medium"
    } else {
        "Low"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn milestone(kind: &str, planned: &str, actual: Option<&str>) -> MilestoneState {
        MilestoneState {
            milestone_type: kind.to_string(),
            planned_date: Some(planned.to_string()),
            actual_date: actual.map(str::to_string),
        }
    }

    #[test]
    fn completed_delay_uses_only_the_highest_threshold() {
        assert_eq!(completed_delay_score(7), 0);
        assert_eq!(completed_delay_score(8), 10);
        assert_eq!(completed_delay_score(14), 10);
        assert_eq!(completed_delay_score(15), 20);
        assert_eq!(completed_delay_score(30), 20);
        assert_eq!(completed_delay_score(31), 30);
    }

    #[test]
    fn risk_levels_use_required_boundaries() {
        assert_eq!(risk_level(0), "Low");
        assert_eq!(risk_level(24), "Low");
        assert_eq!(risk_level(25), "Medium");
        assert_eq!(risk_level(49), "Medium");
        assert_eq!(risk_level(50), "High");
        assert_eq!(risk_level(74), "High");
        assert_eq!(risk_level(75), "Critical");
        assert_eq!(risk_level(100), "Critical");
    }

    #[test]
    fn multiple_overdue_milestones_add_bonus() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 18).expect("valid date");
        let result = evaluate_at(
            None,
            &[
                milestone("SE2", "2026-09-17", None),
                milestone("SE3", "2026-09-10", None),
            ],
            today,
        );
        assert_eq!(result.score, 50);
        assert_eq!(result.level, "High");
        assert_eq!(result.reasons.len(), 3);
    }

    #[test]
    fn hold_adds_risk_but_future_milestone_does_not() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 18).expect("valid date");
        let future_milestones = [milestone("SE4", "2026-09-23", None)];
        let future = evaluate_at(None, &future_milestones, today);
        assert_eq!(future.score, 0);
        assert!(future.reasons.is_empty());
        assert_eq!(
            analyze_milestone(&future_milestones[0], today).status,
            MilestoneStatus::NotCompleted
        );

        let hold = evaluate_at(Some("HOLD"), &[], today);
        assert_eq!(hold.score, 15);
    }

    #[test]
    fn significant_completed_delay_contributes_only_to_risk() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 18).expect("valid date");
        let result = evaluate_at(
            None,
            &[milestone("QGC0", "2026-06-01", Some("2026-06-16"))],
            today,
        );
        assert_eq!(result.score, 20);
        assert_eq!(result.level, "Low");
    }
}
