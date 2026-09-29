use super::{Answer, GroupResult, ProjectResult};
use rusqlite::{params_from_iter, types::Value, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Entity {
    Project,
    Customer,
    Owner,
    PrjStatus,
    RiskLevel,
    Milestone,
    Component,
    NewReleasedComponent,
    System,
    ProductCategory,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Project,
    ProjectId,
    Customer,
    Owner,
    ProjectManager,
    PrjStatus,
    RiskLevel,
    Milestone,
    Component,
    NewReleasedComponent,
    System,
    ProductCategory,
    McrId,
    ProjectName,
    RiskScore,
    PlannedDate,
    ActualDate,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Count,
    CountDistinct,
    List,
    Distinct,
    GroupBy,
    Max,
    Min,
    Top,
    Sort,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Equals,
    Contains,
    StartsWith,
    PersonNameEquals,
    SurnameEquals,
    GreaterThan,
    LessThan,
    IsNull,
    IsNotNull,
    Overdue,
    CompletedLate,
    In,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub field: Field,
    pub operator: Operator,
    pub value: Option<String>,
    #[serde(default)]
    pub values: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sort {
    pub field: Field,
    pub direction: Direction,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredQuery {
    pub entity: Entity,
    pub operation: Operation,
    pub target: Option<Field>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    pub sort: Option<Sort>,
    pub limit: Option<usize>,
}

fn column(field: Field) -> Option<&'static str> {
    match field {
        Field::Project | Field::ProjectId => Some("p.id"),
        Field::ProjectName => Some("p.project_name"),
        Field::McrId => Some("p.mcr_id"),
        Field::Customer => Some("p.customer"),
        Field::Owner => Some("p.project_manager"),
        Field::ProjectManager => Some("p.project_manager"),
        Field::PrjStatus => Some("p.project_status"),
        Field::RiskLevel => Some("r.risk_level"),
        Field::RiskScore => Some("r.risk_score"),
        Field::System => Some("p.system"),
        Field::ProductCategory => Some("p.product_category"),
        Field::Milestone
        | Field::Component
        | Field::NewReleasedComponent
        | Field::PlannedDate
        | Field::ActualDate => None,
    }
}

fn dimension(entity: Entity) -> Option<Field> {
    match entity {
        Entity::Project => Some(Field::Project),
        Entity::Customer => Some(Field::Customer),
        Entity::Owner => Some(Field::Owner),
        Entity::PrjStatus => Some(Field::PrjStatus),
        Entity::RiskLevel => Some(Field::RiskLevel),
        Entity::System => Some(Field::System),
        Entity::ProductCategory => Some(Field::ProductCategory),
        Entity::Milestone => Some(Field::Milestone),
        Entity::Component => Some(Field::Component),
        Entity::NewReleasedComponent => Some(Field::NewReleasedComponent),
    }
}

fn base(entity: Entity) -> &'static str {
    match entity {
        Entity::Milestone => "FROM project_milestones m JOIN projects p ON p.id=m.project_id LEFT JOIN project_risks r ON r.project_id=p.id",
        Entity::Component | Entity::NewReleasedComponent => "FROM project_components c JOIN projects p ON p.id=c.project_id LEFT JOIN project_risks r ON r.project_id=p.id",
        _ => "FROM projects p LEFT JOIN project_risks r ON r.project_id=p.id",
    }
}

fn selected_expression(query: &StructuredQuery, field: Field) -> Result<&'static str, String> {
    match field {
        Field::Milestone if query.entity == Entity::Milestone => Ok("m.milestone_type"),
        Field::Component | Field::NewReleasedComponent
            if matches!(
                query.entity,
                Entity::Component | Entity::NewReleasedComponent
            ) =>
        {
            Ok("c.component_name")
        }
        Field::PlannedDate if query.entity == Entity::Milestone => Ok("m.planned_date"),
        Field::ActualDate if query.entity == Entity::Milestone => Ok("m.actual_date"),
        _ => column(field).ok_or_else(|| "This field is not available for that entity.".into()),
    }
}

fn bind_value(filter: &Filter) -> Result<String, String> {
    let value = filter.value.as_deref().unwrap_or("").trim();
    if value.is_empty() || value.chars().count() > 100 {
        return Err("A filter value must contain 1–100 characters.".into());
    }
    Ok(value.to_string())
}

/// Comparable identity only; the imported/displayed Project Manager value remains unchanged.
pub fn normalize_person_name(value: &str) -> String {
    let before_suffix = value.split(['(', '（']).next().unwrap_or(value);
    before_suffix
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn manager_values(connection: &Connection, filter: &Filter) -> Result<Vec<String>, String> {
    if filter.field != Field::ProjectManager {
        return Err("Person-name operators require Project Manager.".into());
    }
    let requested = normalize_person_name(&bind_value(filter)?);
    if requested.is_empty() {
        return Err("A person name is required.".into());
    }
    let mut statement = connection.prepare("SELECT DISTINCT project_manager FROM projects WHERE project_manager IS NOT NULL AND TRIM(project_manager)<>''").map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut matches = Vec::new();
    for row in rows {
        let original = row.map_err(|e| e.to_string())?;
        let normalized = normalize_person_name(&original);
        let matched = if filter.operator == Operator::SurnameEquals {
            normalized.split_whitespace().next() == Some(requested.as_str())
        } else {
            normalized == requested
        };
        if matched {
            matches.push(original);
        }
        if matches.len() > 500 {
            return Err("Too many matching manager variants; refine the name.".into());
        }
    }
    Ok(matches)
}

fn predicate(
    connection: &Connection,
    query: &StructuredQuery,
    filter: &Filter,
    values: &mut Vec<Value>,
) -> Result<String, String> {
    let related = match filter.field {
        Field::Milestone | Field::PlannedDate | Field::ActualDate => {
            Some(("project_milestones m", "m"))
        }
        Field::Component | Field::NewReleasedComponent => Some(("project_components c", "c")),
        _ => None,
    };
    let same_base = match filter.field {
        Field::Milestone | Field::PlannedDate | Field::ActualDate => {
            query.entity == Entity::Milestone
        }
        Field::Component | Field::NewReleasedComponent => matches!(
            query.entity,
            Entity::Component | Entity::NewReleasedComponent
        ),
        _ => true,
    };
    let field_expr = if let Some((_, alias)) = related {
        match filter.field {
            Field::Milestone => format!("{alias}.milestone_type"),
            Field::Component | Field::NewReleasedComponent => format!("{alias}.component_name"),
            Field::PlannedDate => format!("{alias}.planned_date"),
            Field::ActualDate => format!("{alias}.actual_date"),
            _ => unreachable!(),
        }
    } else {
        selected_expression(query, filter.field)?.to_string()
    };
    let condition = match filter.operator {
        Operator::In => {
            if filter.field != Field::ProjectId {
                return Err("IN is only supported for project IDs.".into());
            }
            let ids = filter.values.as_ref().ok_or("Project ID set is missing.")?;
            if ids.is_empty() || ids.len() > 50 || ids.iter().any(|id| *id <= 0) {
                return Err("Project ID set must contain 1–50 positive IDs.".into());
            }
            values.extend(ids.iter().copied().map(Value::Integer));
            format!("p.id IN ({})", vec!["?"; ids.len()].join(","))
        }
        Operator::Overdue | Operator::CompletedLate => {
            if filter.field != Field::Milestone {
                return Err("Delay status applies only to milestones.".into());
            }
            let mut condition = if filter.operator == Operator::Overdue {
                "m.planned_date < date('now','localtime') AND m.actual_date IS NULL".to_string()
            } else {
                "m.planned_date IS NOT NULL AND m.actual_date > m.planned_date".to_string()
            };
            if filter.value.as_ref().is_some_and(|v| !v.trim().is_empty()) {
                let stage = bind_value(filter)?;
                if !["SE2", "SE3", "SE4", "QGC0", "QGC4"].contains(&stage.to_uppercase().as_str()) {
                    return Err("Unknown milestone stage.".into());
                }
                values.push(Value::Text(stage));
                condition.push_str(" AND UPPER(m.milestone_type)=UPPER(?)");
            }
            condition
        }
        Operator::IsNull => format!("({field_expr} IS NULL OR TRIM({field_expr})='')"),
        Operator::IsNotNull => format!("({field_expr} IS NOT NULL AND TRIM({field_expr})<>'')"),
        Operator::Equals => {
            values.push(Value::Text(bind_value(filter)?));
            format!("LOWER(TRIM({field_expr}))=LOWER(TRIM(?))")
        }
        Operator::Contains => {
            values.push(Value::Text(bind_value(filter)?));
            format!("INSTR(LOWER({field_expr}),LOWER(?))>0")
        }
        Operator::StartsWith => {
            let value = bind_value(filter)?;
            values.push(Value::Text(value.clone()));
            values.push(Value::Text(value));
            format!("SUBSTR(LOWER({field_expr}),1,LENGTH(?))=LOWER(?)")
        }
        Operator::PersonNameEquals | Operator::SurnameEquals => {
            let matches = manager_values(connection, filter)?;
            if matches.is_empty() {
                "0=1".into()
            } else {
                values.extend(matches.iter().cloned().map(Value::Text));
                format!(
                    "p.project_manager IN ({})",
                    vec!["?"; matches.len()].join(",")
                )
            }
        }
        Operator::GreaterThan | Operator::LessThan => {
            if !matches!(
                filter.field,
                Field::RiskScore | Field::PlannedDate | Field::ActualDate
            ) {
                return Err("Range comparison is not supported for this field.".into());
            }
            let value = bind_value(filter)?;
            if filter.field == Field::RiskScore {
                let score: i64 = value.parse().map_err(|_| "Risk score must be numeric.")?;
                if !(0..=100).contains(&score) {
                    return Err("Risk score must be between 0 and 100.".into());
                }
                values.push(Value::Integer(score));
            } else {
                values.push(Value::Text(value));
            }
            format!(
                "{field_expr} {} ?",
                if filter.operator == Operator::GreaterThan {
                    ">"
                } else {
                    "<"
                }
            )
        }
    };
    if let Some((table, _)) = related.filter(|_| !same_base) {
        let extra = if filter.field == Field::NewReleasedComponent {
            " AND c.component_type='new_released'"
        } else {
            ""
        };
        Ok(format!(
            "EXISTS (SELECT 1 FROM {table} WHERE {}.project_id=p.id{extra} AND {condition})",
            if table.starts_with("project_milestones") {
                "m"
            } else {
                "c"
            }
        ))
    } else if filter.field == Field::NewReleasedComponent {
        Ok(format!("c.component_type='new_released' AND {condition}"))
    } else {
        Ok(condition)
    }
}

fn conditions(
    connection: &Connection,
    query: &StructuredQuery,
) -> Result<(String, Vec<Value>), String> {
    if query.filters.len() > 12 {
        return Err("Too many filters in one question.".into());
    }
    let mut values = Vec::new();
    let mut parts = Vec::new();
    if query.entity == Entity::NewReleasedComponent {
        parts.push("c.component_type='new_released'".into());
    }
    for filter in &query.filters {
        parts.push(format!(
            "({})",
            predicate(connection, query, filter, &mut values)?
        ));
    }
    Ok((
        if parts.is_empty() {
            "1=1".into()
        } else {
            parts.join(" AND ")
        },
        values,
    ))
}

fn target(query: &StructuredQuery) -> Result<&'static str, String> {
    let field = query
        .target
        .or_else(|| dimension(query.entity))
        .ok_or("Missing query target.")?;
    selected_expression(query, field)
}

fn project_rows(
    connection: &Connection,
    query: &StructuredQuery,
    where_sql: &str,
    values: &[Value],
    max: usize,
) -> Result<Vec<ProjectResult>, String> {
    let order = match &query.sort {
        Some(sort) => {
            let field = selected_expression(query, sort.field)?;
            format!(
                "{field} {} , p.id",
                if matches!(sort.direction, Direction::Asc) {
                    "ASC"
                } else {
                    "DESC"
                }
            )
        }
        None => "r.risk_score DESC, p.project_name COLLATE NOCASE, p.id".into(),
    };
    let sql = format!("SELECT DISTINCT p.id,p.project_name,p.mcr_id,COALESCE(p.project_status,'Unspecified'),COALESCE(r.risk_level,'Low'),COALESCE(r.risk_score,0),r.primary_risk_driver {} WHERE {where_sql} ORDER BY {order} LIMIT {max}", base(query.entity));
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(params_from_iter(values.iter()), |row| {
            Ok(ProjectResult {
                id: row.get(0)?,
                project_name: row.get(1)?,
                mcr_id: row.get(2)?,
                project_status: row.get(3)?,
                risk_level: row.get(4)?,
                risk_score: row.get(5)?,
                primary_risk_driver: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub fn matching_project_ids(
    connection: &Connection,
    query: &StructuredQuery,
    limit: usize,
) -> Result<(i64, Vec<i64>), String> {
    let (where_sql, values) = conditions(connection, query)?;
    let total: i64 = connection
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT p.id) {} WHERE {where_sql}",
                base(query.entity)
            ),
            params_from_iter(values.iter()),
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let rows = project_rows(connection, query, &where_sql, &values, limit.min(50))?;
    Ok((total, rows.into_iter().map(|row| row.id).collect()))
}

pub fn answer(connection: &Connection, query: &StructuredQuery) -> Result<Answer, String> {
    let mut effective = query.clone();
    if matches!(
        query.operation,
        Operation::GroupBy | Operation::Distinct | Operation::CountDistinct
    ) && query.entity == Entity::Project
    {
        effective.entity = match query.target {
            Some(Field::Milestone | Field::PlannedDate | Field::ActualDate) => Entity::Milestone,
            Some(Field::Component) => Entity::Component,
            Some(Field::NewReleasedComponent) => Entity::NewReleasedComponent,
            _ => Entity::Project,
        };
    }
    let (where_sql, values) = conditions(connection, &effective)?;
    let from = base(effective.entity);
    let limit = query.limit.unwrap_or(50).clamp(1, 50);
    let selected = target(&effective)?;
    match query.operation {
        Operation::Count | Operation::CountDistinct => {
            if query.operation == Operation::CountDistinct && query.entity == Entity::Owner {
                let sql = format!("SELECT DISTINCT p.project_manager {from} WHERE {where_sql} AND p.project_manager IS NOT NULL AND TRIM(p.project_manager)<>''");
                let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
                let rows = statement
                    .query_map(params_from_iter(values.iter()), |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(|e| e.to_string())?;
                let mut people = HashSet::new();
                for row in rows {
                    people.insert(normalize_person_name(&row.map_err(|e| e.to_string())?));
                }
                return Ok(Answer::Metric {
                    summary: format!("{} distinct project managers match.", people.len()),
                    value: people.len() as i64,
                });
            }
            let expression = if matches!(query.operation, Operation::CountDistinct)
                || query.entity == Entity::Project
            {
                format!("COUNT(DISTINCT {selected})")
            } else {
                format!("COUNT({selected})")
            };
            let value: i64 = connection
                .query_row(
                    &format!("SELECT {expression} {from} WHERE {where_sql}"),
                    params_from_iter(values.iter()),
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            let summary = query
                .filters
                .iter()
                .find(|f| {
                    f.field == Field::ProjectManager && f.operator == Operator::PersonNameEquals
                })
                .and_then(|f| f.value.as_deref())
                .map(|name| format!("{} currently owns {value} projects.", name.trim()))
                .unwrap_or_else(|| {
                    format!(
                        "{value} matching {} records.",
                        format!("{:?}", query.entity).to_lowercase()
                    )
                });
            Ok(Answer::Metric { summary, value })
        }
        Operation::Max | Operation::Min => {
            if query.target != Some(Field::RiskScore) {
                return Err("MAX and MIN currently support risk score only.".into());
            }
            let value: i64 = connection
                .query_row(
                    &format!(
                        "SELECT COALESCE({}({selected}),0) {from} WHERE {where_sql}",
                        if query.operation == Operation::Max {
                            "MAX"
                        } else {
                            "MIN"
                        }
                    ),
                    params_from_iter(values.iter()),
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            Ok(Answer::Metric {
                summary: format!(
                    "The {} matching risk score is {value}.",
                    if query.operation == Operation::Max {
                        "highest"
                    } else {
                        "lowest"
                    }
                ),
                value,
            })
        }
        Operation::GroupBy => {
            if !matches!(
                query.target,
                Some(
                    Field::Customer
                        | Field::Owner
                        | Field::PrjStatus
                        | Field::RiskLevel
                        | Field::System
                        | Field::ProductCategory
                        | Field::Milestone
                        | Field::Component
                        | Field::NewReleasedComponent
                )
            ) {
                return Err("Unsupported grouping field.".into());
            }
            let sql = format!("SELECT COALESCE(NULLIF(TRIM({selected}),''),'Unassigned') AS label,COUNT(DISTINCT p.id) AS n {from} WHERE {where_sql} GROUP BY label ORDER BY n DESC,label COLLATE NOCASE LIMIT {limit}");
            let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = statement
                .query_map(params_from_iter(values.iter()), |row| {
                    Ok(GroupResult {
                        label: row.get(0)?,
                        count: row.get(1)?,
                    })
                })
                .map_err(|e| e.to_string())?;
            Ok(Answer::GroupedSummary {
                summary: "Matching projects grouped by the requested field, ranked by count."
                    .into(),
                groups: rows
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?,
            })
        }
        Operation::Distinct => {
            if selected == "p.id" {
                return Err("Use LIST for project records.".into());
            }
            let sql = format!("SELECT DISTINCT TRIM({selected}) {from} WHERE {where_sql} AND {selected} IS NOT NULL AND TRIM({selected})<>'' ORDER BY 1 COLLATE NOCASE LIMIT {limit}");
            let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = statement
                .query_map(params_from_iter(values.iter()), |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|e| e.to_string())?;
            Ok(Answer::TextSummary {
                summary: "Distinct values in the current dataset.".into(),
                items: rows
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?,
            })
        }
        Operation::List | Operation::Top | Operation::Sort => {
            if query.entity != Entity::Project {
                return Err("Listing this entity is not supported; ask for distinct values or group counts.".into());
            }
            let (total, _) = matching_project_ids(connection, query, 0)?;
            let projects = project_rows(connection, query, &where_sql, &values, limit)?;
            let shown = projects.len();
            Ok(Answer::ProjectList {
                summary: if total as usize > shown {
                    format!("{total} matching projects. Showing the first {shown}.")
                } else {
                    format!("{total} matching projects.")
                },
                total,
                projects,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    fn fixture() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        migrations::run(&mut db).unwrap();
        db.execute_batch(r#"
            INSERT INTO projects(id,mcr_id,project_name,customer,project_manager,project_status,system,product_category,source_file,source_row)
            VALUES (1,'BM-1','Alpha','ABC','Wu Ming','WORK','A','P','x.xlsx',2),
                   (2,'BM-2','Beta','ABC','Wu Li','WORK','A','P','x.xlsx',3),
                   (3,'BM-3','Gamma','XYZ','Li Hua','HOLD','B','Q','x.xlsx',4);
            INSERT INTO project_risks(project_id,risk_score,risk_level)
            VALUES (1,90,'Critical'),(2,30,'Medium'),(3,85,'Critical');
            INSERT INTO project_milestones(project_id,milestone_type,planned_date,actual_date,plan_source,actual_source)
            VALUES (1,'SE3',date('now','localtime','-2 day'),NULL,'MCR','Project List'),
                   (2,'SE3',date('now','localtime','-1 day'),NULL,'MCR','Project List'),
                   (3,'SE3','2024-01-01','2024-01-05','MCR','Project List');
            INSERT INTO project_components(project_id,component_name,component_type)
            VALUES (1,'ECU','existing'),(2,'ECU','new_released'),(3,'Rail','existing');
        "#).unwrap();
        db
    }

    fn query(entity: Entity, operation: Operation, filters: Vec<Filter>) -> StructuredQuery {
        StructuredQuery {
            entity,
            operation,
            target: None,
            filters,
            sort: None,
            limit: None,
        }
    }
    fn filter(field: Field, operator: Operator, value: Option<&str>) -> Filter {
        Filter {
            field,
            operator,
            value: value.map(str::to_string),
            values: None,
        }
    }
    fn metric(answer: Answer) -> i64 {
        if let Answer::Metric { value, .. } = answer {
            value
        } else {
            panic!("expected metric")
        }
    }

    #[test]
    fn counts_distinct_owners_by_prefix_and_combines_status_with_overdue_stage() {
        let db = fixture();
        let owners = query(
            Entity::Owner,
            Operation::CountDistinct,
            vec![filter(Field::Owner, Operator::StartsWith, Some("Wu"))],
        );
        assert_eq!(metric(answer(&db, &owners).unwrap()), 2);
        let projects = query(
            Entity::Project,
            Operation::Count,
            vec![
                filter(Field::PrjStatus, Operator::Equals, Some("WORK")),
                filter(Field::Milestone, Operator::Overdue, Some("SE3")),
            ],
        );
        assert_eq!(metric(answer(&db, &projects).unwrap()), 2);
    }

    #[test]
    fn groups_critical_projects_by_owner_and_filters_customer_and_risk() {
        let db = fixture();
        let mut grouped = query(
            Entity::Project,
            Operation::GroupBy,
            vec![filter(Field::RiskLevel, Operator::Equals, Some("Critical"))],
        );
        grouped.target = Some(Field::Owner);
        grouped.limit = Some(1);
        assert!(
            matches!(answer(&db, &grouped).unwrap(), Answer::GroupedSummary { groups, .. } if groups.len()==1 && groups[0].label=="Li Hua" || groups.len()==1 && groups[0].label=="Wu Ming")
        );
        let listed = query(
            Entity::Project,
            Operation::List,
            vec![
                filter(Field::Customer, Operator::Equals, Some("ABC")),
                filter(Field::RiskLevel, Operator::Equals, Some("Critical")),
            ],
        );
        assert!(
            matches!(answer(&db, &listed).unwrap(), Answer::ProjectList { total:1, projects, .. } if projects[0].id==1)
        );
    }

    #[test]
    fn distinct_components_extrema_and_parameter_binding() {
        let db = fixture();
        let new_components = query(Entity::NewReleasedComponent, Operation::Distinct, vec![]);
        assert!(
            matches!(answer(&db, &new_components).unwrap(), Answer::TextSummary { items, .. } if items==["ECU"])
        );
        let mut maximum = query(Entity::Project, Operation::Max, vec![]);
        maximum.target = Some(Field::RiskScore);
        assert_eq!(metric(answer(&db, &maximum).unwrap()), 90);
        let injected = query(
            Entity::Project,
            Operation::Count,
            vec![filter(
                Field::Customer,
                Operator::Equals,
                Some("ABC' OR 1=1 --"),
            )],
        );
        assert_eq!(metric(answer(&db, &injected).unwrap()), 0);
        let invalid = query(
            Entity::Project,
            Operation::Count,
            vec![filter(
                Field::RiskScore,
                Operator::GreaterThan,
                Some("0 OR 1=1"),
            )],
        );
        assert!(answer(&db, &invalid).is_err());
    }

    #[test]
    fn selected_project_ids_are_exact_count_but_bounded_sample() {
        let db = fixture();
        let scoped = query(
            Entity::Project,
            Operation::List,
            vec![filter(Field::Owner, Operator::StartsWith, Some("Wu"))],
        );
        assert_eq!(matching_project_ids(&db, &scoped, 1).unwrap(), (2, vec![1]));
    }

    #[test]
    fn previous_project_id_sets_are_parameterized_and_bounded() {
        let db = fixture();
        let scoped = StructuredQuery {
            entity: Entity::Project,
            operation: Operation::Count,
            target: None,
            filters: vec![Filter {
                field: Field::ProjectId,
                operator: Operator::In,
                value: None,
                values: Some(vec![1, 3]),
            }],
            sort: None,
            limit: None,
        };
        assert_eq!(metric(answer(&db, &scoped).unwrap()), 2);
        let mut invalid = scoped;
        invalid.filters[0].values = Some(vec![]);
        assert!(answer(&db, &invalid).is_err());
    }

    #[test]
    fn supports_range_null_late_component_grouping_and_ordered_top() {
        let db = fixture();
        let above = query(
            Entity::Project,
            Operation::Count,
            vec![filter(Field::RiskScore, Operator::GreaterThan, Some("84"))],
        );
        assert_eq!(metric(answer(&db, &above).unwrap()), 2);
        let late = query(
            Entity::Project,
            Operation::Count,
            vec![filter(
                Field::Milestone,
                Operator::CompletedLate,
                Some("SE3"),
            )],
        );
        assert_eq!(metric(answer(&db, &late).unwrap()), 1);
        let missing_actual = query(
            Entity::Project,
            Operation::Count,
            vec![filter(Field::ActualDate, Operator::IsNull, None)],
        );
        assert_eq!(metric(answer(&db, &missing_actual).unwrap()), 2);
        let mut grouped = query(Entity::Project, Operation::GroupBy, vec![]);
        grouped.target = Some(Field::Component);
        assert!(
            matches!(answer(&db, &grouped).unwrap(), Answer::GroupedSummary { groups, .. } if groups[0].label=="ECU" && groups[0].count==2)
        );
        let mut top = query(Entity::Project, Operation::Top, vec![]);
        top.sort = Some(Sort {
            field: Field::RiskScore,
            direction: Direction::Desc,
        });
        top.limit = Some(1);
        assert!(
            matches!(answer(&db, &top).unwrap(), Answer::ProjectList { total:3, projects, .. } if projects[0].id==1)
        );
    }

    #[test]
    fn full_person_name_matches_organization_suffix_but_not_longer_name() {
        let mut db = Connection::open_in_memory().unwrap();
        migrations::run(&mut db).unwrap();
        db.execute_batch(
            r#"
            INSERT INTO projects(id,mcr_id,project_name,project_manager,source_file,source_row)
            VALUES (1,'BM-1','A','WU Wei (RBXX/SM3)','x.xlsx',2),
                   (2,'BM-2','B','WU Wei','x.xlsx',3),
                   (3,'BM-3','C','WU Weidong','x.xlsx',4),
                   (4,'BM-4','D','ZHANG Wei','x.xlsx',5),
                   (5,'BM-5','E','CHEN Bin','x.xlsx',6);
        "#,
        )
        .unwrap();
        let person = query(
            Entity::Project,
            Operation::Count,
            vec![filter(
                Field::ProjectManager,
                Operator::PersonNameEquals,
                Some("wu wei"),
            )],
        );
        for typed in ["wu wei", "WU Wei", "Wu Wei", "wU wEi"] {
            let mut variant = person.clone();
            variant.filters[0].value = Some(typed.into());
            assert_eq!(metric(answer(&db, &variant).unwrap()), 2);
        }
        let listed = query(Entity::Project, Operation::List, person.filters.clone());
        assert!(
            matches!(answer(&db, &listed).unwrap(), Answer::ProjectList { total:2, projects, .. } if projects.len()==2 && projects.iter().all(|p| p.id<=2))
        );
        let surname = query(
            Entity::Owner,
            Operation::CountDistinct,
            vec![filter(
                Field::ProjectManager,
                Operator::SurnameEquals,
                Some("WU"),
            )],
        );
        assert_eq!(metric(answer(&db, &surname).unwrap()), 2);
        let zhang = query(
            Entity::Project,
            Operation::Count,
            vec![filter(
                Field::ProjectManager,
                Operator::PersonNameEquals,
                Some("zhang wei"),
            )],
        );
        assert_eq!(metric(answer(&db, &zhang).unwrap()), 1);
        let chen = query(
            Entity::Project,
            Operation::Count,
            vec![filter(
                Field::ProjectManager,
                Operator::PersonNameEquals,
                Some("CHEN Bin"),
            )],
        );
        assert_eq!(metric(answer(&db, &chen).unwrap()), 1);
        let missing = query(
            Entity::Project,
            Operation::Count,
            vec![filter(
                Field::ProjectManager,
                Operator::PersonNameEquals,
                Some("WU Weix"),
            )],
        );
        assert_eq!(metric(answer(&db, &missing).unwrap()), 0);
        assert_eq!(normalize_person_name(" wU  wEi （RBXX/SM3）"), "WU WEI");
    }
}
