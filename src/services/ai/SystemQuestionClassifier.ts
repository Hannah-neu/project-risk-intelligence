export type SystemQuestionIntent =
  | "risk_methodology"
  | "milestone_methodology"
  | "data_source_explanation"
  | "import_methodology"
  | "mcr_update_methodology"
  | "metric_definition"
  | "ai_behavior_explanation";

export function classifySystemQuestion(question: string): SystemQuestionIntent | null {
  const q = question.toLowerCase().trim();
  if (!q || q.length > 500) return null;
  if (/^(?:how many|count\b|number of)\b/.test(q)) return null;
  // Requests for project records belong to the data-query/analytical route.
  if (/哪些项目|which projects|列出项目|show projects/.test(q)) return null;
  const method = /怎么|如何|为什么|是什么|什么区别|是否|一定|标准|定义|计算|处理|来源|来自|哪些字段|哪些问题|会不会|会调用|取最新|重复|影响|why |how |which fields|which questions|what questions|what data|where |definition|defined|calculated|source|difference|affect|updated by|happens if|maximum/.test(q);
  if (!method) return null;
  if (/\b(ai|llm)\b|enterprise llm|本地查询|完整 excel|完整数据|回答的数据|调用.*模型|调用.*llm/.test(q)) return "ai_behavior_explanation";
  if (/total projects|overdue projects|critical projects|on track|项目总数|逾期项目|严重风险项目|kpi|指标/.test(q) && /怎么|如何|定义|计算|口径|how.*calculat|how.*defined|definition/.test(q)) return "metric_definition";
  if (/公式|#n\/a|formula|hidden rows|filters|隐藏行|筛选|components|逗号|分号|prj mcr-id.*重复|duplicate.*mcr.id/.test(q)) return "import_methodology";
  if (/mcr|level 0|主 id|子 id|同阶段|匹配不到|match a project/.test(q) && /更新|计划日期|日期|主 id|子 id|level 0|match|updated|fields/.test(q)) return "mcr_update_methodology";
  if (/来自哪里|来源|导入的|系统计算|where does|where is|source|which fields/.test(q) && /se[234]|qgc[04]|prj status|里程碑|plan|actual|字段|fields/.test(q)) return "data_source_explanation";
  if (/里程碑|milestone|overdue|completed late|actual|delay|延迟|逾期|实际.*空/.test(q)) return "milestone_methodology";
  if (/风险|risk|critical|high|medium|low|分数|score/.test(q) && /分数|score|定义|标准|计算|maximum|等级|risk level/.test(q) && !/第一个项目|这个项目|该项目|which project|why is [a-z0-9_-]+ (critical|high|risky)/.test(q)) return "risk_methodology";
  if (/导入|import/.test(q)) return "import_methodology";
  return null;
}
