use crate::{enterprise_config, importer, repositories::dashboard, risk_engine};
use rusqlite::Connection;

pub fn answer(
    connection: &Connection,
    intent: &str,
    question: &str,
    language: &str,
) -> Result<String, String> {
    if question.trim().is_empty() || question.chars().count() > 500 {
        return Err("Enter a question of 1–500 characters.".into());
    }
    let zh = language == "zh";
    let q = question.to_lowercase();
    let result = match intent {
        "risk_methodology" => risk(&q, zh),
        "milestone_methodology" => milestone(&q, zh),
        "data_source_explanation" => data_source(&q, zh),
        "import_methodology" => import(&q, zh),
        "mcr_update_methodology" => mcr(&q, zh),
        "metric_definition" => metric(&q, zh),
        "ai_behavior_explanation" => ai(connection, &q, zh)?,
        _ => return Err("Unsupported explanation intent.".into()),
    };
    Ok(result)
}

fn risk(q: &str, zh: bool) -> String {
    let r = risk_engine::COMPLETED_LATE_POINTS;
    if q.contains("critical")
        || q.contains("high")
        || q.contains("medium")
        || q.contains("low")
        || q.contains("标准")
    {
        let level = if q.contains("critical") {
            "Critical"
        } else if q.contains("high") {
            "High"
        } else if q.contains("medium") {
            "Medium"
        } else if q.contains("low") {
            "Low"
        } else {
            ""
        };
        let bands = format!(
            "Low 0–{}, Medium {}–{}, High {}–{}, Critical {}–{}",
            risk_engine::MEDIUM_RISK_MIN - 1,
            risk_engine::MEDIUM_RISK_MIN,
            risk_engine::HIGH_RISK_MIN - 1,
            risk_engine::HIGH_RISK_MIN,
            risk_engine::CRITICAL_RISK_MIN - 1,
            risk_engine::CRITICAL_RISK_MIN,
            risk_engine::MAX_RISK_SCORE
        );
        return if zh {
            format!("{level} 风险级别按计算后的 Risk Score 划分：{bands}。PRJ Status 是独立的导入业务字段。")
        } else {
            format!("{level} risk is determined from the calculated Risk Score: {bands}. PRJ Status is a separate imported business field.")
        };
    }
    if q.contains("maximum") || q.contains("最大") || q.contains("上限") {
        return if zh {
            format!(
                "Risk Score 上限为 {}：风险引擎累加各项分数后用上限截断，因此不会超过 {}。",
                risk_engine::MAX_RISK_SCORE,
                risk_engine::MAX_RISK_SCORE
            )
        } else {
            format!(
                "The risk engine caps the accumulated score at {}, so Risk Score cannot exceed {}.",
                risk_engine::MAX_RISK_SCORE,
                risk_engine::MAX_RISK_SCORE
            )
        };
    }
    if zh {
        format!("Risk Score 从 0 开始：每个逾期里程碑 +{}；已完成但延迟 {}–{} 天 +{}、{}–{} 天 +{}、{} 天以上 +{}；两个及以上逾期再 +{}；PRJ Status 为 HOLD 再 +{}。最终封顶 {}。计划日期将近但尚未逾期不会加分。",
            risk_engine::OVERDUE_POINTS, r[0].0, r[1].0-1, r[0].1, r[1].0, r[2].0-1, r[1].1, r[2].0, r[2].1, risk_engine::MULTIPLE_OVERDUE_POINTS, risk_engine::HOLD_POINTS, risk_engine::MAX_RISK_SCORE)
    } else {
        format!("Risk Score starts at 0: +{} per overdue milestone; completed delays of {}–{} days +{}, {}–{} days +{}, and {}+ days +{}; +{} for two or more overdue milestones; +{} if PRJ Status is HOLD. The result is capped at {}. A future milestone approaching its date adds no risk by itself.",
            risk_engine::OVERDUE_POINTS, r[0].0, r[1].0-1, r[0].1, r[1].0, r[2].0-1, r[1].1, r[2].0, r[2].1, risk_engine::MULTIPLE_OVERDUE_POINTS, risk_engine::HOLD_POINTS, risk_engine::MAX_RISK_SCORE)
    }
}

fn milestone(q: &str, zh: bool) -> String {
    if q.contains("actual") || q.contains("实际") {
        return if zh {
            "Actual 为空不一定异常：有未来计划日期时是 Not completed；计划日期早于本地今天才是 Overdue；计划和实际都为空是 Not planned。".into()
        } else {
            "A blank Actual is not necessarily abnormal: a future plan is Not completed; a plan before local today is Overdue; no plan and no actual is Not planned.".into()
        };
    }
    if q.contains("difference") || q.contains("区别") {
        return if zh {
            "Completed late 表示已有 Actual，且 Actual 晚于 Plan；Overdue 表示 Plan 早于本地今天，但 Actual 仍为空。两者不是同一种状态。".into()
        } else {
            "Completed late means Actual exists and is after Plan. Overdue means Plan is before local today while Actual is still blank.".into()
        };
    }
    if q.contains("delay") || q.contains("延迟") || q.contains("delta") {
        return if zh {
            "已完成里程碑的 Delta Days = Actual − Plan（天）；逾期未完成的 Delta Days = 本地今天 − Plan；其他未完成或未计划状态无延迟天数。".into()
        } else {
            "For completed milestones, Delta Days = Actual − Plan in days. For overdue unfinished milestones, it is local today − Plan. Other unfinished or unplanned milestones have no delay value.".into()
        };
    }
    if zh {
        "Overdue = 计划日期早于本地今天，且实际日期为空；等于今天或未来的未完成计划不算逾期。里程碑状态还有 Completed on time、Completed late、Not completed 和 Not planned。".into()
    } else {
        "Overdue means planned_date is before local today and actual_date is null. A plan due today or later is Not completed, not Overdue. Other statuses are Completed on time, Completed late, and Not planned.".into()
    }
}

fn data_source(q: &str, zh: bool) -> String {
    if q.contains("prj status") {
        return if zh {
            "PRJ Status 来自 Project List Excel 的同名列，导入时会修剪空白并统一大写；它不是系统计算的 Risk Level。".into()
        } else {
            "PRJ Status comes from the Project List Excel column of that name, normalized by trimming and uppercasing. It is not the calculated Risk Level.".into()
        };
    }
    let stage = importer::MILESTONES
        .iter()
        .find(|(stage, _, _)| q.contains(&stage.to_lowercase()));
    let Some((name, plan, actual)) = stage else {
        return if zh {
            "SE2、SE3、SE4、QGC0、QGC4 的 Plan 和 Actual 最初来自 Project List 对应列；MCR 只可能覆盖已匹配项目的 Plan，Actual 不由 MCR 更新。".into()
        } else {
            "SE2, SE3, SE4, QGC0 and QGC4 Plan and Actual initially come from their Project List columns. MCR may override Plan for matched projects; it never updates Actual.".into()
        };
    };
    if q.contains("actual") || q.contains("实际") {
        if zh {
            format!("{name} Actual 来自 Project List 的“{actual}”列；MCR 更新不会改 Actual。")
        } else {
            format!("{name} Actual comes from the Project List '{actual}' column; MCR updates never change Actual.")
        }
    } else if zh {
        format!("{name} Plan 初始来自 Project List 的“{plan}”列；匹配的 MCR 更新可覆盖它。项目详情里的 Plan Source 可显示当前来源。")
    } else {
        format!("{name} Plan initially comes from the Project List '{plan}' column and may be overridden by a matching MCR update. Plan Source on Project Detail shows the current source.")
    }
}

fn import(q: &str, zh: bool) -> String {
    if q.contains("公式") || q.contains("formula") {
        return if zh {
            "导入器使用 XLSX 文件内已有的公式缓存结果，不自行计算公式。缓存缺失或不可用会产生 Import Issue；必填的 PRJ MCR-ID 为 ERROR 并阻止导入，可选字段可置空并给出 WARNING。".into()
        } else {
            "The importer uses stored formula cache results from the workbook; it does not calculate formulas. Missing or unusable cached values create Import Issues: required PRJ MCR-ID blocks import with an ERROR, while optional fields may become null with a WARNING.".into()
        };
    }
    if q.contains("#n/a") || q.contains("excel error") {
        return if zh {
            "#N/A 等 Excel 错误值不会当作普通文本导入；按字段规则规范为空并生成 Import Issue。必填 PRJ MCR-ID 为 ERROR，可选字段通常为 WARNING。".into()
        } else {
            "Excel errors such as #N/A are not imported as ordinary text. They become null with an Import Issue: ERROR for required PRJ MCR-ID and normally WARNING for optional fields.".into()
        };
    }
    if q.contains("components") || q.contains("逗号") || q.contains("分号") {
        let separators: String = importer::COMPONENT_SEPARATORS.iter().collect();
        return if zh {
            format!("Components 使用这些分隔符拆分：{separators}；会修剪空白、去掉空项和重复项，再用英文逗号加空格保存。斜杠不作为分隔符，遇到时保留并提示检查。")
        } else {
            format!("Components splits on these separators: {separators}. Items are trimmed, empty and duplicate items removed, then joined with comma-space. A slash is preserved and flagged for review.")
        };
    }
    if q.contains("filter") || q.contains("hidden") || q.contains("隐藏") || q.contains("筛选")
    {
        return if zh {
            "导入器扫描工作簿各表寻找表头，并读取表头后的实际单元格行；Excel 筛选状态或隐藏行不会自动排除数据。".into()
        } else {
            "The importer scans worksheets for the required header and reads physical cell rows beneath it. Excel filters and hidden rows do not automatically exclude data.".into()
        };
    }
    if zh {
        "PRJ MCR-ID 是项目的必填匹配键，规范化后重复会产生 ERROR 并阻止替换，以免多个项目共享同一 ID 而导致 MCR 更新指向不明确。".into()
    } else {
        "PRJ MCR-ID is the required project matching key. Duplicate normalized IDs cause an ERROR and block replacement, because MCR updates would otherwise be ambiguous.".into()
    }
}

fn mcr(q: &str, zh: bool) -> String {
    if q.contains("match") || q.contains("匹配不到") {
        return if zh {
            "MCR 的 MCR ID Level 0 若找不到当前 Project List 的 PRJ MCR-ID，会给出 WARNING 并跳过该主 ID；不会新建项目。".into()
        } else {
            "If an MCR Level 0 ID has no matching PRJ MCR-ID in the current Project List, the importer warns and skips it; it never creates a new project.".into()
        };
    }
    if q.contains("最新") || q.contains("latest") || q.contains("多个日期") {
        return if zh {
            "同一 MCR ID Level 0 + 阶段可能有多行；导入器在可用日期中取最晚日期，与行顺序无关，然后更新该阶段的计划日期。".into()
        } else {
            "For multiple rows with the same MCR Level 0 ID and stage, the importer selects the latest valid date independently of row order, then updates that stage's planned date.".into()
        };
    }
    if q.contains("子 id") || q.contains("child") || q.contains("主 id") {
        return if zh {
            "匹配项目只用 MCR ID Level 0 对应 PRJ MCR-ID；子 ID（PRJ MCR ID K）仅保留追溯用途。多个子 ID 的同阶段记录会按主 ID 和阶段汇总，并取最晚日期。".into()
        } else {
            "Projects are matched only by MCR ID Level 0 against PRJ MCR-ID. Child ID (PRJ MCR ID K) is traceability only. Rows are aggregated by Level 0 ID and stage, choosing the latest date.".into()
        };
    }
    let stages = importer::mcr::STAGES.join(", ");
    let excluded = importer::mcr::EXCLUDED_STAGES.join(", ");
    if zh {
        format!("MCR 用 MCR ID Level 0 匹配现有 PRJ MCR-ID，仅更新 {stages} 的 planned_date 和 plan_source（MCR），不更新 Actual，也不新建项目；{excluded} 不参与映射。更新后重算风险和洞察。")
    } else {
        format!("MCR matches Level 0 ID to existing PRJ MCR-ID and updates only planned_date and plan_source (MCR) for {stages}. It does not change Actual or create projects; {excluded} is excluded. Risks and insights are recalculated afterward.")
    }
}

fn metric(q: &str, zh: bool) -> String {
    let key = if q.contains("overdue") || q.contains("逾期") {
        "overdue"
    } else if q.contains("critical") || q.contains("严重") {
        "critical"
    } else if q.contains("on track") {
        "on_track"
    } else {
        "total"
    };
    dashboard::metric_definition(key, zh)
        .unwrap_or_default()
        .to_string()
}

fn ai(connection: &Connection, q: &str, zh: bool) -> Result<String, String> {
    let config = enterprise_config::view(connection)?;
    if q.contains("完整") || q.contains("full") || q.contains("excel") {
        return Ok(if zh {
            "不会发送完整 Excel。Enterprise LLM 仅在启用并有密钥时收到针对问题构建的精简 SQLite 上下文；宽泛问题只附带统计和部分高风险项目，内容大小有上限。".into()
        } else {
            "The full Excel is not sent. When enabled and configured, Enterprise LLM receives only a compact SQLite context selected for the question; broad questions include summaries and a limited top-risk subset, with a size limit.".into()
        });
    }
    if q.contains("来源") || q.contains("source") {
        return Ok(if zh {
            "本地事实查询来自当前 SQLite 的导入项目、里程碑及计算风险；方法说明依据应用内风险、导入、MCR 和 KPI 规则。Enterprise LLM 的分析使用经过筛选的 SQLite 上下文，而不是完整 Excel。".into()
        } else {
            "Local factual answers use imported projects, milestones and calculated risks in current SQLite. Method explanations follow application rules. Enterprise LLM analysis uses selected SQLite context, not the whole workbook.".into()
        });
    }
    let status = if config.provider == "llm" && config.has_api_key {
        if zh {
            "已启用"
        } else {
            "enabled"
        }
    } else if zh {
        "未启用"
    } else {
        "not enabled"
    };
    Ok(if zh {
        format!("项目数量、筛选、聚合及这些方法说明优先走本地确定性查询。分析性问题、对话追问，以及启用时本地不支持的问题才可能调用 Enterprise LLM；当前 Enterprise LLM {status}。")
    } else {
        format!("Counts, filters, aggregations and these methodology explanations use deterministic local handling. Analytical questions, conversational follow-ups, and unsupported local questions when configured may use Enterprise LLM; it is currently {status}.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explanations_follow_rule_constants_and_mapping() {
        assert!(risk("风险分数怎么计算", true).contains(&risk_engine::OVERDUE_POINTS.to_string()));
        assert!(
            risk("critical 怎么定义", true).contains(&risk_engine::CRITICAL_RISK_MIN.to_string())
        );
        assert!(data_source("qgc0 actual 来源", true).contains(importer::MILESTONES[3].2));
        assert!(mcr("which fields are updated by mcr", false).contains("planned_date"));
        assert!(metric("overdue projects", false).contains("distinct projects"));
    }
}
