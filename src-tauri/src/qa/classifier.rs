#[derive(Debug, PartialEq, Eq)]
pub enum Intent {
    TotalProjects,
    RiskCount(&'static str),
    Projects(ProjectFilter),
    HighRiskByCustomer,
    CriticalByManager,
    MainRiskDrivers,
    Unsupported,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ProjectFilter {
    Risk(&'static str),
    Overdue(Option<&'static str>),
    HighestRisk,
    Hold,
    Customer(String),
    Manager(String),
    CustomerRisk(String, &'static str),
    ManagerRisk(String, &'static str),
    McrId(String),
    Ecu,
    NewComponents(Option<&'static str>),
}

pub fn classify(question: &str) -> Intent {
    let normalized = question.trim().to_lowercase();
    let q = normalized
        .trim_end_matches(['?', '？', '.', '。', '!', '！'])
        .trim();
    let count = q.starts_with("how many ")
        || q.starts_with("count ")
        || q.contains("多少")
        || q.contains("数量");
    let list = q.starts_with("which projects")
        || q.starts_with("show projects")
        || q.starts_with("list projects")
        || q.contains("哪些项目");
    if q.contains(" not ") || q.contains(" no ") || q.contains("without ") || q.contains("不包含")
    {
        return Intent::Unsupported;
    }

    if q.contains("main risk driver") || q.contains("top risk driver") || q.contains("主要风险原因")
    {
        return Intent::MainRiskDrivers;
    }
    if (q.contains("customer") || q.contains("客户"))
        && (q.contains("most high-risk")
            || q.contains("most high risk")
            || q.contains("高风险最多"))
    {
        return Intent::HighRiskByCustomer;
    }
    if (q.contains("project manager") || q.contains("项目经理"))
        && q.contains("critical")
        && !q.contains("managed by")
    {
        return Intent::CriticalByManager;
    }
    for (prefix, risk) in [
        ("show critical projects for customer ", "Critical"),
        ("show high risk projects for customer ", "High"),
        ("show medium risk projects for customer ", "Medium"),
        ("show low risk projects for customer ", "Low"),
        ("show critical projects managed by ", "Critical"),
        ("show high risk projects managed by ", "High"),
        ("show medium risk projects managed by ", "Medium"),
        ("show low risk projects managed by ", "Low"),
    ] {
        if let Some(value) = q.strip_prefix(prefix).and_then(clean_value) {
            return if prefix.contains("customer") {
                Intent::Projects(ProjectFilter::CustomerRisk(value, risk))
            } else {
                Intent::Projects(ProjectFilter::ManagerRisk(value, risk))
            };
        }
    }
    if let Some(value) = q
        .strip_prefix("show project with mcr-id ")
        .and_then(clean_value)
    {
        return Intent::Projects(ProjectFilter::McrId(value));
    }
    for prefix in [
        "show projects for customer ",
        "projects for customer ",
        "show projects of customer ",
        "which projects belong to customer ",
        "which projects are for customer ",
    ] {
        if let Some(value) = q.strip_prefix(prefix).and_then(clean_value) {
            return Intent::Projects(ProjectFilter::Customer(value));
        }
    }
    for prefix in [
        "show projects managed by ",
        "projects managed by ",
        "show projects for project manager ",
        "which projects are managed by ",
    ] {
        if let Some(value) = q.strip_prefix(prefix).and_then(clean_value) {
            return Intent::Projects(ProjectFilter::Manager(value));
        }
    }
    if list
        && (q.contains("newly released") || q.contains("new released") || q.contains("新发布组件"))
    {
        let component = ["ecu", "rail", "injector", "pump", "dnox"]
            .into_iter()
            .find(|name| q.contains(name));
        return Intent::Projects(ProjectFilter::NewComponents(component));
    }
    if list
        && (q.contains("contain") || q.contains("have") || q.contains("包含"))
        && q.contains("ecu")
    {
        return Intent::Projects(ProjectFilter::Ecu);
    }
    if list && (q.contains("overdue") || q.contains("逾期")) {
        let stage = if q.contains("se2") {
            Some("SE2")
        } else if q.contains("se3") {
            Some("SE3")
        } else if q.contains("se4") {
            Some("SE4")
        } else if q.contains("qgc0") {
            Some("QGC0")
        } else if q.contains("qgc4") {
            Some("QGC4")
        } else {
            None
        };
        return Intent::Projects(ProjectFilter::Overdue(stage));
    }
    if list
        && (q.contains("highest risk score")
            || q.contains("highest risk")
            || q.contains("风险分数最高"))
    {
        return Intent::Projects(ProjectFilter::HighestRisk);
    }
    if list && (q.contains("hold") || q.contains("暂停项目")) {
        return Intent::Projects(ProjectFilter::Hold);
    }
    if (count || list) && (q.contains("critical") || q.contains("严重风险")) {
        return if count {
            Intent::RiskCount("Critical")
        } else {
            Intent::Projects(ProjectFilter::Risk("Critical"))
        };
    }
    if (count || list)
        && (q.contains("high risk") || q.contains("high-risk") || q.contains("高风险"))
    {
        return if count {
            Intent::RiskCount("High")
        } else {
            Intent::Projects(ProjectFilter::Risk("High"))
        };
    }
    for (term, level) in [("medium risk", "Medium"), ("low risk", "Low")] {
        if (count || list) && q.contains(term) {
            return if count {
                Intent::RiskCount(level)
            } else {
                Intent::Projects(ProjectFilter::Risk(level))
            };
        }
    }
    if [
        "how many projects are there",
        "how many projects",
        "count projects",
        "total projects",
        "项目总数",
        "一共有多少项目",
    ]
    .contains(&q)
    {
        return Intent::TotalProjects;
    }
    Intent::Unsupported
}

fn clean_value(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches(['\'', '"', '“', '”', '‘', '’'])
        .trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_questions_are_classified() {
        assert_eq!(
            classify("How many projects are there?"),
            Intent::TotalProjects
        );
        assert_eq!(
            classify("How many projects are Critical?"),
            Intent::RiskCount("Critical")
        );
        assert_eq!(
            classify("Which projects have overdue SE3?"),
            Intent::Projects(ProjectFilter::Overdue(Some("SE3")))
        );
        assert_eq!(
            classify("Show projects for customer Bosch."),
            Intent::Projects(ProjectFilter::Customer("bosch".into()))
        );
        assert_eq!(
            classify("Show projects managed by Sarah Chen"),
            Intent::Projects(ProjectFilter::Manager("sarah chen".into()))
        );
        assert_eq!(
            classify("Which projects belong to Customer Bosch?"),
            Intent::Projects(ProjectFilter::Customer("bosch".into()))
        );
        assert_eq!(
            classify("Which projects are managed by Sarah Chen?"),
            Intent::Projects(ProjectFilter::Manager("sarah chen".into()))
        );
        assert_eq!(
            classify("Which projects have overdue QGC4?"),
            Intent::Projects(ProjectFilter::Overdue(Some("QGC4")))
        );
        assert_eq!(
            classify("Which projects are Medium Risk?"),
            Intent::Projects(ProjectFilter::Risk("Medium"))
        );
        assert_eq!(classify("What will happen next year?"), Intent::Unsupported);
        assert_eq!(
            classify("How many projects are due this month?"),
            Intent::Unsupported
        );
        assert_eq!(
            classify("Which projects are not Critical?"),
            Intent::Unsupported
        );
    }
}
