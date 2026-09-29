export const terminology = {
  project: ["project", "projects", "项目"],
  customer: ["customer", "客户"],
  manager: ["project manager", "owner", "项目经理", "负责人", "负责"],
  risk: ["risk", "风险"],
  critical: ["critical", "严重风险", "关键风险"],
  high: ["high risk", "high-risk", "高风险"],
  medium: ["medium risk", "中风险"],
  low: ["low risk", "低风险"],
  overdue: ["overdue", "逾期", "超期"],
  milestone: ["milestone", "里程碑", "节点"],
  status: ["status", "状态"],
  component: ["component", "components", "零部件", "部件"],
  hold: ["hold", "暂停"],
  ecu: ["ecu"],
  stages: ["SE2", "SE3", "SE4", "QGC0", "QGC4"],
  analytical: ["why", "explain", "recommend", "recommendation", "recommendations", "should", "prioritize", "prioritise", "pay attention", "summarize", "summarise", "summary", "pattern", "patterns", "trend", "trends", "compare", "analyze", "analyse", "insight", "insights", "root cause", "为什么", "建议", "优先", "关注", "总结", "概述", "规律", "趋势", "分析", "洞察", "值得注意"],
  negation: ["not", "no", "without", "不包含", "不是", "没有", "非"],
  followup: ["what about", "how about", "those", "them", "these", "that", "it", "and", "那", "这些", "它们", "还有"]
} as const;

export type Term = Exclude<keyof typeof terminology, "stages">;

export function hasTerm(text: string, term: Term): boolean {
  return terminology[term].some((alias) => {
    const haystack = text.toLowerCase();
    const needle = alias.toLowerCase();
    if (!/^[a-z -]+$/i.test(alias)) return haystack.includes(needle);
    let position = haystack.indexOf(needle);
    while (position >= 0) {
      const before = haystack[position - 1] ?? "";
      const after = haystack[position + needle.length] ?? "";
      if (!/[a-z]/.test(before) && !/[a-z]/.test(after)) return true;
      position = haystack.indexOf(needle, position + 1);
    }
    return false;
  });
}

export function findStage(text: string): string | undefined {
  return terminology.stages.find((stage) => new RegExp(`\\b${stage}\\b`, "i").test(text));
}
