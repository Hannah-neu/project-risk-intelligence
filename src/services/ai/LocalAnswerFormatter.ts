import type { QaAnswer } from "../../types/qa";
import type { QuestionLanguage } from "./QuestionNormalizer";
import type { StructuredQuery } from "./StructuredQuery";

export function formatLocalAnswer(result: QaAnswer, question: string, language: QuestionLanguage, query?: StructuredQuery): string {
  if (language === "en") return result.summary;

  switch (result.type) {
    case "metric":
      if (query?.entity === "project") {
        const name = query.filters.find((filter) => filter.field === "project_manager" && filter.operator === "person_name_equals")?.value;
        if (name) return query.filters.length === 1 ? `${name} 当前负责 ${result.value} 个项目。` : `${name} 当前有 ${result.value} 个符合条件的项目。`;
      }
      if (query && query.entity !== "project") return `符合条件的${({ owner: "负责人", customer: "客户", milestone: "里程碑", component: "组件", new_released_component: "新发布组件", system: "系统", product_category: "产品类别", prj_status: "项目状态", risk_level: "风险等级" } as Record<string, string>)[query.entity] ?? "记录"}有 ${result.value} 个。`;
      if (query && (query.operation === "max" || query.operation === "min")) return `符合条件的${query.operation === "max" ? "最高" : "最低"}风险分数是 ${result.value}。`;
      if (query && query.filters.length > 0 && !/critical|high risk|medium risk|low risk/i.test(question)) return `符合条件的项目有 ${result.value} 个。`;
      if (/critical/i.test(question)) return `当前共有 ${result.value} 个 Critical 项目。`;
      if (/high risk/i.test(question)) return `当前共有 ${result.value} 个 High Risk 项目。`;
      if (/medium risk/i.test(question)) return `当前共有 ${result.value} 个 Medium Risk 项目。`;
      if (/low risk/i.test(question)) return `当前共有 ${result.value} 个 Low Risk 项目。`;
      return `当前数据集共有 ${result.value} 个项目。`;
    case "project_list": {
      if (result.total === 0) return "未找到符合条件的项目。";
      const manager = query?.filters.find((filter) => filter.field === "project_manager" && filter.operator === "person_name_equals")?.value;
      if (manager) return `${manager} 当前${query?.filters.length === 1 ? "负责" : "有"} ${result.total} 个${query?.filters.length === 1 ? "" : "符合条件的"}项目${result.projects.length < result.total ? `，显示前 ${result.projects.length} 个` : ""}。`;
      const subject = /critical/i.test(question) ? "Critical 项目" : /high risk/i.test(question) ? "High Risk 项目" : /medium risk/i.test(question) ? "Medium Risk 项目" : /low risk/i.test(question) ? "Low Risk 项目" : /overdue/i.test(question) ? "逾期项目" : "符合条件的项目";
      return result.projects.length < result.total
        ? `找到 ${result.total} 个${subject}，显示风险分数最高的前 ${result.projects.length} 个。`
        : `当前共有 ${result.total} 个${subject}。`;
    }
    case "grouped_summary":
      return result.groups.length ? "以下是当前数据集的分组统计：" : "未找到符合条件的分组。";
    case "text_summary":
      if (query?.operation === "distinct") return result.items.length ? "当前数据集中的不同取值：" : "没有符合条件的取值。";
      return result.items.length ? "当前数据集中的主要风险原因：" : "当前数据集中没有记录主要风险原因。";
    case "unsupported_question":
      return "当前本地查询尚不支持这个问题。";
  }
}

export function formatLocalItems(items: string[], language: QuestionLanguage): string[] {
  if (language === "en") return items;
  return items.map((item) => item.replace(/ — (\d+) projects?$/, " — $1 个项目"));
}
