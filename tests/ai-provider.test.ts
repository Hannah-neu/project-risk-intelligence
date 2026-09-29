import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ProjectAIAnalysis } from "../src/components/projects/ProjectAIAnalysis";
import { Sidebar } from "../src/components/layout/Sidebar";
import { AskProjectData, MarkdownAnswer } from "../src/components/insights/AskProjectData";
import { LLMProvider } from "../src/services/ai/LLMProvider";
import { AgentQuestionRouter } from "../src/services/ai/AgentQuestionRouter";
import { LocalQueryProvider } from "../src/services/ai/LocalQueryProvider";
import { normalizeQuestion } from "../src/services/ai/QuestionNormalizer";
import { QuestionRouter, classifyQuestion } from "../src/services/ai/QuestionRouter";
import { parseStructuredQuery } from "../src/services/ai/StructuredQuery";
import { extractPersonName, normalizePersonName } from "../src/services/ai/personName";
import { SystemExplanationProvider } from "../src/services/ai/SystemExplanationProvider";
import { validateSemanticPlan } from "../src/services/ai/SemanticInterpreter";
import { assessStructuredQuery } from "../src/services/ai/QueryCompletenessGate";
import { getAIProvider } from "../src/services/ai/providerFactory";
import type { AIProvider } from "../src/services/ai/AIProvider";
import type { QaAnswer } from "../src/types/qa";

test("local provider is the default and selected by the factory", () => {
  assert.ok(getAIProvider() instanceof LocalQueryProvider);
  assert.equal(getAIProvider(), getAIProvider("local"));
  assert.ok(getAIProvider("llm") instanceof LLMProvider);
});

test("Project Detail AI analysis renders an explicit on-demand action without generating on mount", () => {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const markup = renderToStaticMarkup(
    createElement(QueryClientProvider, { client: queryClient },
      createElement(MemoryRouter, null, createElement(ProjectAIAnalysis, { projectId: 7 })))
  );
  assert.match(markup, /Generate AI Analysis/);
  assert.match(markup, /English/);
  assert.match(markup, /中文/);
  assert.equal(queryClient.isMutating(), 0);
  assert.doesNotMatch(markup, /Regenerate Analysis/);
  const source = readFileSync("src/components/projects/ProjectAIAnalysis.tsx", "utf8");
  assert.match(source, /to="\/settings">Open AI Configuration/);
});

test("connection testing requires the same saved Enterprise provider selection as analysis", () => {
  const settings = readFileSync("src/pages/Settings.tsx", "utf8");
  const backend = readFileSync("src-tauri/src/commands/enterprise.rs", "utf8");
  assert.match(settings, /config\.provider !== "llm"/);
  assert.match(backend, /test_enterprise_connection[\s\S]*enterprise_credentials\(&database\)/);
});

test("Overview replaces its old cards with risk charts and Risks navigation is removed", () => {
  const dashboard = readFileSync("src/pages/Dashboard.tsx", "utf8");
  const routes = readFileSync("src/App.tsx", "utf8");
  const navigation = renderToStaticMarkup(createElement(MemoryRouter, null, createElement(Sidebar)));
  assert.match(dashboard, /DashboardRiskPanels/);
  assert.doesNotMatch(dashboard, /ProjectPortfolio|AIRiskSummary/);
  assert.match(routes, /path="risks" element=\{<Navigate to="\/" replace \/>\}/);
  assert.doesNotMatch(navigation, /href="\/risks"/);
});

test("AI Insights keeps only the local summary and Ask Project Data panels", () => {
  const page = readFileSync("src/pages/AIInsights.tsx", "utf8");
  assert.match(page, /<PortfolioInsightSummary /);
  assert.match(page, /<AskProjectData \/>/);
  assert.doesNotMatch(page, /<TopRiskDrivers|<RuleSignals|<AttentionProjects|<MilestoneDelayAnalysis|<RiskConcentration/);
});

test("methodology questions use seven dedicated local intents without calling either query or LLM provider", async () => {
  const examples = [
    ["风险分数怎么计算？", "risk_methodology"], ["Critical 是怎么定义的？", "risk_methodology"],
    ["High Risk 的标准是什么？", "risk_methodology"], ["Why is the maximum risk score 100?", "risk_methodology"],
    ["Overdue 怎么定义？", "milestone_methodology"], ["Completed late 和 Overdue 有什么区别？", "milestone_methodology"],
    ["Actual 为空一定代表异常吗？", "milestone_methodology"], ["How is milestone delay calculated?", "milestone_methodology"],
    ["SE3 plan 来自哪里？", "data_source_explanation"], ["PRJ Status 是导入的还是系统计算的？", "data_source_explanation"],
    ["Where does QGC0 actual come from?", "data_source_explanation"],
    ["公式单元格怎么处理？", "import_methodology"], ["#N/A 怎么处理？", "import_methodology"],
    ["Components 的逗号和分号怎么处理？", "import_methodology"], ["PRJ MCR-ID 为什么不能重复？", "import_methodology"],
    ["Do filters or hidden rows affect import?", "import_methodology"],
    ["MCR 怎么更新计划日期？", "mcr_update_methodology"], ["为什么一个主 ID 下要看多个子 ID？", "mcr_update_methodology"],
    ["同阶段多个日期为什么取最新？", "mcr_update_methodology"], ["Which fields are updated by MCR?", "mcr_update_methodology"],
    ["What happens if an MCR Level 0 ID cannot match a project?", "mcr_update_methodology"],
    ["Total Projects 怎么算？", "metric_definition"], ["Overdue Projects 怎么算？", "metric_definition"],
    ["Critical Projects 怎么算？", "metric_definition"],
    ["哪些问题走本地查询？", "ai_behavior_explanation"], ["哪些问题会调用 Enterprise LLM？", "ai_behavior_explanation"],
    ["会不会把完整 Excel 发给 LLM？", "ai_behavior_explanation"], ["AI 回答的数据来源是什么？", "ai_behavior_explanation"]
  ] as const;
  const seen: string[] = [];
  const fail: AIProvider = { async askQuestion() { throw new Error("data query or LLM must not be called"); } };
  const explanations = new SystemExplanationProvider(async (intent, question, language) => { seen.push(`${intent}:${language}:${question}`); return "method answer"; });
  const router = new QuestionRouter(fail, fail, async () => true, explanations);
  for (const [question, intent] of examples) {
    assert.equal(classifyQuestion(question), intent, question);
    assert.deepEqual(router.route(question), { category: intent, provider: "local" }, question);
    const answer = await router.askQuestion(question);
    assert.equal(answer.content, "method answer");
    assert.equal(answer.provider, "local");
  }
  assert.equal(seen.length, examples.length);
});

test("local provider preserves STEP 11 metric, list, grouping, text and unsupported answers", async () => {
  const responses: QaAnswer[] = [
    { type: "metric", summary: "3 projects are Critical risk.", value: 3 },
    { type: "project_list", summary: "1 project is overdue.", total: 1, projects: [{ id: 7, projectName: "Alpha", mcrId: "MCR-7", projectStatus: "WORK", riskLevel: "Critical", riskScore: 90, primaryRiskDriver: "Overdue SE3" }] },
    { type: "grouped_summary", summary: "Customers ranked by risk.", groups: [{ label: "Bosch", count: 2 }] },
    { type: "text_summary", summary: "Main risk drivers.", items: ["Overdue SE3 — 2 projects"] },
    { type: "unsupported_question", summary: "I can't answer this question.", suggestions: ["How many projects are Critical?"] }
  ];
  const questions: string[] = [];
  let version = "project-list:1:mcr:0";
  const provider = new LocalQueryProvider(
    async (question) => { questions.push(question); return responses.shift()!; },
    async () => version
  );

  const metric = await provider.askQuestion("How many projects are Critical?");
  assert.deepEqual(metric, { content: "3 projects are Critical risk.", answerType: "metric", provider: "local", language: "en", value: 3, metadata: { datasetVersion: version } });

  version = "project-list:1:mcr:1";
  const projects = await provider.askQuestion("Which projects are overdue?", { datasetVersion: "project-list:1:mcr:0" });
  assert.equal(projects.answerType, "project_list");
  assert.deepEqual(projects.referencedProjectIds, [7]);
  assert.equal(projects.metadata?.datasetVersion, version);
  if (projects.answerType === "project_list") assert.equal(projects.projects[0].primaryRiskDriver, "Overdue SE3");

  assert.equal((await provider.askQuestion("Which customers have the most high-risk projects?")).answerType, "grouped_summary");
  assert.equal((await provider.askQuestion("What are the main risk drivers?")).answerType, "text");
  assert.equal((await provider.askQuestion("Forecast the budget")).answerType, "unsupported");
  assert.deepEqual(questions, ["How many projects are Critical?", "Which projects are overdue?", "Which customers have the most high-risk projects?", "What are the main risk drivers?", "Forecast the budget"]);
});

test("LLM provider calls the backend and preserves language and model metadata", async () => {
  const calls: string[] = [];
  const provider = new LLMProvider(async (question, language) => {
    calls.push(`${language}:${question}`);
    return { text: "测试成功", model: "qwen3.6-flash", inputTokens: 12, outputTokens: 3 };
  });
  const answer = await provider.askQuestion("请分析风险", { language: "zh" });
  assert.equal(answer.content, "测试成功");
  assert.equal(answer.provider, "llm");
  assert.equal(answer.language, "zh");
  assert.equal(answer.metadata?.model, "qwen3.6-flash");
  assert.deepEqual(calls, ["zh:请分析风险"]);
});

test("LLM provider forwards only short follow-up references to the backend client", async () => {
  let observed: unknown;
  const provider = new LLMProvider(async (_question, _language, context) => {
    observed = context;
    return { text: "Phoenix ERP has an overdue SE3 milestone.", model: "qwen3.6-flash" };
  });
  await provider.askQuestion("为什么第一个项目风险最高？", { language: "zh", lastProjectIds: [7], datasetVersion: "project-list:1:mcr:0", recentMessages: [{ role: "user", content: "哪些项目是 Critical？" }] });
  assert.deepEqual(observed, { language: "zh", lastProjectIds: [7], datasetVersion: "project-list:1:mcr:0", recentMessages: [{ role: "user", content: "哪些项目是 Critical？" }] });
});

test("LLM provider presents backend errors without object stringification", async () => {
  const provider = new LLMProvider(async () => { throw { code: "not_configured", message: "Enterprise LLM is not selected in Settings." }; });
  await assert.rejects(provider.askQuestion("Summarize risks"), /not selected in Settings/);
});

test("Enterprise client invokes Rust with bounded follow-up identifiers, not project records", () => {
  const client = readFileSync("src/services/ai/enterpriseClient.ts", "utf8");
  assert.match(client, /invoke<EnterpriseReply>\("invoke_enterprise_llm"/);
  assert.match(client, /lastProjectIds.*slice\(0, 3\)/);
  assert.doesNotMatch(client, /projectRecords|fullDataset/);
});

test("router selects the correct provider for factual, filter and aggregation questions", async () => {
  const calls: string[] = [];
  const local: AIProvider = { async askQuestion(question) { calls.push(`local:${question}`); return { content: "local answer", answerType: "metric", provider: "local", value: 2 }; } };
  const llm: AIProvider = { async askQuestion(question) { calls.push(`llm:${question}`); return { content: "not configured", answerType: "text", provider: "llm", items: [] }; } };
  const router = new QuestionRouter(local, llm);

  for (const [question, category] of [
    ["How many Critical projects are there?", "factual_query"],
    ["Which projects are overdue?", "project_filter"],
    ["Which projects belong to Customer Bosch?", "project_filter"],
    ["Which projects are managed by Alice?", "project_filter"],
    ["Which projects contain ECU?", "project_filter"],
    ["Which projects are HOLD?", "project_filter"],
    ["Which customers have the most high-risk projects?", "aggregation"],
    ["What are the main risk drivers?", "aggregation"]
  ] as const) {
    assert.deepEqual(router.route(question), { category, provider: "local" });
    assert.equal((await router.askQuestion(question)).content, "local answer");
  }
  assert.equal(calls.length, 8);
  assert.ok(calls.every((call) => call.startsWith("local:")));
});

test("analytical questions and contextual follow-ups route to the Enterprise backend", async () => {
  const router = new QuestionRouter(undefined, new LLMProvider(async () => ({ text: "Enterprise answer", model: "qwen3.6-flash" })), async () => true);
  for (const question of [
    "Which projects should I pay attention to and why?",
    "Summarize the main portfolio risks.",
    "What patterns do you see across delayed projects?"
  ]) {
    assert.deepEqual(router.route(question), { category: "analytical_question", provider: "llm" });
    const answer = await router.askQuestion(question);
    assert.equal(answer.provider, "llm");
    assert.equal(answer.content, "Enterprise answer");
  }
  const context = { lastIntent: "project_list" as const, lastProjectIds: [7] };
  assert.deepEqual(router.route("What about those?", context), { category: "conversational_followup", provider: "llm" });
  assert.equal((await router.askQuestion("What about those?", context)).provider, "llm");
});

test("unsupported questions stay controlled when Enterprise is disabled", async () => {
  const fail: AIProvider = { async askQuestion() { throw new Error("provider must not be called"); } };
  const router = new QuestionRouter(fail, fail, async () => false);
  for (const question of ["", "How many projects are due this month?", "Which projects are not Critical?", "Forecast next year's budget"]) {
    assert.equal(classifyQuestion(question), "unsupported");
    const answer = await router.askQuestion(question);
    assert.equal(answer.answerType, "unsupported");
    assert.match(answer.content, /not supported yet/);
  }
});

test("structured parser generalizes entities, operations, AND filters and bilingual examples", () => {
  const parse = (question: string) => parseStructuredQuery(normalizeQuestion(question));
  assert.deepEqual(parse("姓 Wu 的工程师有几个？"), {
    entity: "owner", operation: "count_distinct", target: undefined,
    filters: [{ field: "project_manager", operator: "surname_equals", value: "Wu" }], sort: undefined, limit: 50
  });
  assert.deepEqual(parse("Customer ABC 有哪些 Critical 项目？")?.filters, [
    { field: "risk_level", operator: "equals", value: "Critical" },
    { field: "customer", operator: "equals", value: "ABC" }
  ]);
  const grouped = parse("哪个 Owner 的 Critical 项目最多？");
  assert.equal(grouped?.operation, "group_by");
  assert.equal(grouped?.target, "owner");
  assert.equal(grouped?.limit, 1);
  assert.deepEqual(parse("WORK 状态且 SE3 overdue 的项目有几个？")?.filters, [
    { field: "prj_status", operator: "equals", value: "WORK" },
    { field: "milestone", operator: "overdue", value: "SE3" }
  ]);
  assert.equal(parse("List distinct customers")?.operation, "distinct");
  assert.equal(parse("How many overdue milestones?")?.operation, "count");
  assert.equal(classifyQuestion("How many overdue milestones?"), "factual_query");
  assert.deepEqual(parse("Which projects have newly released Rail components?")?.filters, [{ field: "new_released_component", operator: "contains", value: "Rail" }]);
  assert.deepEqual(parse("List projects where customer contains Bosch")?.filters, [{ field: "customer", operator: "contains", value: "Bosch" }]);
  assert.equal(parse("Top 5 projects by risk score")?.operation, "top");
  assert.equal(parse("What is the maximum risk score?")?.operation, "max");
  assert.deepEqual(parseStructuredQuery(normalizeQuestion("Why are projects owned by Alice risky?"), true)?.filters, [{ field: "project_manager", operator: "person_name_equals", value: "Alice" }]);
  assert.equal(parse("Which projects are not Critical?"), null);
});

test("router sends structured facts locally and analysis scope to the existing LLM provider", async () => {
  const seen: unknown[] = [];
  const local: AIProvider = { async askQuestion(_question, context) { seen.push(context?.structuredQuery); return { content: "2", answerType: "metric", provider: "local", value: 2 }; } };
  const llm: AIProvider = { async askQuestion(_question, context) { seen.push(context?.structuredQuery); return { content: "analysis", answerType: "text", provider: "llm", items: [] }; } };
  const router = new QuestionRouter(local, llm, async () => true);
  assert.equal((await router.askQuestion("WORK 状态且 SE3 overdue 的项目有几个？")).provider, "local");
  assert.equal((seen[0] as { operation: string }).operation, "count");
  assert.equal((await router.askQuestion("姓 Wu 的工程师负责的项目为什么风险较高？")).provider, "llm");
  assert.deepEqual((seen[1] as { filters: unknown[] }).filters, [{ field: "project_manager", operator: "surname_equals", value: "Wu" }]);
});

test("full manager names are never dropped from project counts or lists", () => {
  const countQuestions = ["目前 wu wei 有几个项目", "WU Wei 负责几个项目？", "项目经理 WU Wei 负责哪些项目？", "How many projects does WU Wei own?"];
  for (const question of countQuestions) {
    const parsed = parseStructuredQuery(normalizeQuestion(question));
    assert.equal(parsed?.entity, "project", question);
    assert.deepEqual(parsed?.filters, [{ field: "project_manager", operator: "person_name_equals", value: /wu wei/i.test(question) && question.startsWith("目前") ? "wu wei" : "WU Wei" }], question);
  }
  const list = parseStructuredQuery(normalizeQuestion("WU Wei 有哪些项目？"));
  assert.equal(list?.operation, "list");
  assert.deepEqual(list?.filters, [{ field: "project_manager", operator: "person_name_equals", value: "WU Wei" }]);
  assert.deepEqual(parseStructuredQuery(normalizeQuestion("Which projects are managed by WU Wei?"))?.filters, list?.filters);
  const surname = parseStructuredQuery(normalizeQuestion("姓 WU 的项目经理有几个？"));
  assert.equal(surname?.entity, "owner");
  assert.equal(surname?.operation, "count_distinct");
  assert.deepEqual(surname?.filters, [{ field: "project_manager", operator: "surname_equals", value: "WU" }]);
  assert.equal(normalizePersonName("WU Wei (RBXX/SM3)"), "WU WEI");
  assert.equal(normalizePersonName(" wU  wEi （RBXX/SM3）"), "WU WEI");
  assert.equal(extractPersonName("目前 ZHANG Wei 有几个项目"), "ZHANG Wei");
  assert.equal(extractPersonName("CHEN Bin 有哪些项目？"), "CHEN Bin");
  assert.equal(extractPersonName("目前 WANG Feng 的项目有几个？"), "WANG Feng");
  assert.equal(parseStructuredQuery(normalizeQuestion("How many projects are associated with WU Wei?")), null);
});

test("WU Wei count reaches structured SQLite with a manager filter and no broad fallback", async () => {
  let received: unknown;
  const local = new LocalQueryProvider(
    async () => { throw new Error("legacy unfiltered count must not run"); },
    async () => "project-list:1:mcr:0",
    async (query) => { received = query; return { type: "metric", summary: "wu wei currently owns 2 projects.", value: 2 }; }
  );
  const fail: AIProvider = { async askQuestion() { throw new Error("LLM must not answer factual counts"); } };
  const router = new QuestionRouter(local, fail, async () => true);
  const result = await router.askQuestion("目前 wu wei 有几个项目");
  assert.equal(result.provider, "local");
  assert.equal(result.content, "wu wei 当前负责 2 个项目。");
  assert.deepEqual((received as { filters: unknown[] }).filters, [{ field: "project_manager", operator: "person_name_equals", value: "wu wei" }]);
  const unsafe = await router.askQuestion("目前 WU Wei 有几个项目或其他项目？");
  assert.equal(unsafe.answerType, "unsupported");
});

test("unsupported factual questions do not fall back to the LLM", async () => {
  const calls: string[] = [];
  const local: AIProvider = { async askQuestion(question) { calls.push(`local:${question}`); return { content: "unsupported", answerType: "unsupported", provider: "local", suggestions: [] }; } };
  const llm: AIProvider = { async askQuestion(question) { calls.push(`llm:${question}`); return { content: "contextual answer", answerType: "text", provider: "llm", items: [] }; } };
  const router = new QuestionRouter(local, llm, async () => true);
  assert.equal((await router.askQuestion("How many projects are Critical?")).provider, "local");
  assert.deepEqual(calls, ["local:How many projects are Critical"]);
  calls.length = 0;
  assert.equal((await router.askQuestion("Which projects look unusual?")).provider, "llm");
  assert.deepEqual(calls, ["llm:Which projects look unusual?"]);
  calls.length = 0;
  assert.equal((await router.askQuestion("Which projects are not Critical?")).answerType, "unsupported");
  assert.deepEqual(calls, []);
});

test("STEP 13B analytical examples and first-project follow-up route to LLM", () => {
  const router = new QuestionRouter(undefined, undefined, async () => true);
  for (const question of ["哪些项目最值得关注，为什么？", "总结当前项目风险。", "Which projects look unusual?", "Which projects require attention?", "哪些 Critical project 最值得关注？"]) {
    assert.equal(router.route(question).provider, "llm");
  }
  assert.equal(router.route("为什么第一个项目风险最高？", { lastIntent: "project_list", lastProjectIds: [7] }).provider, "llm");
});

test("English, Chinese and mixed questions converge on the same local query", () => {
  const pairs = [
    ["How many projects are there?", "有多少个项目？", "How many projects are there?"],
    ["How many projects are Critical?", "有多少个严重风险项目？", "How many projects are Critical?"],
    ["Which projects are Critical?", "哪些项目是严重风险？", "Which projects are Critical?"],
    ["Which projects are overdue?", "哪些项目逾期了？", "Which projects are overdue?"],
    ["Which projects have overdue SE3?", "哪些项目的 SE3 已经逾期？", "Which projects have overdue SE3?"],
    ["Which projects are on HOLD?", "哪些项目处于 HOLD 状态？", "Which projects are on HOLD?"],
    ["Which projects contain ECU?", "哪些项目包含 ECU？", "Which projects contain ECU?"]
  ] as const;
  for (const [english, chinese, canonical] of pairs) {
    assert.equal(normalizeQuestion(english).canonical, canonical.slice(0, -1));
    assert.equal(normalizeQuestion(chinese).canonical, canonical.slice(0, -1));
    assert.equal(classifyQuestion(english), classifyQuestion(chinese));
    assert.equal(normalizeQuestion(chinese).language, "zh");
  }
  assert.equal(normalizeQuestion("哪些 project 是 Critical？").canonical, "Which projects are Critical");
  assert.equal(normalizeQuestion("有多少个 Critical 项目？").canonical, "How many projects are Critical");
  assert.equal(normalizeQuestion("项目总数是多少？").canonical, "How many projects are there");
  assert.equal(normalizeQuestion("有哪些超期项目？").canonical, "Which projects are overdue");
  assert.equal(normalizeQuestion("哪些项目处于暂停状态？").canonical, "Which projects are on HOLD");
  assert.equal(normalizeQuestion("哪些项目 SE4 overdue？").canonical, "Which projects have overdue SE4");
  assert.equal(normalizeQuestion("Customer ABC 有哪些 High Risk 项目？").canonical, "Show High Risk projects for customer ABC");
  assert.equal(normalizeQuestion("Zhang San owner 的项目有哪些？").canonical, "Show projects managed by Zhang San");
  assert.equal(normalizeQuestion("张三负责哪些 Critical 项目？").canonical, "Show Critical projects managed by 张三");
});

test("normalization preserves literal customer, manager, MCR-ID and milestone entities", () => {
  assert.equal(normalizeQuestion("客户 Bosch 有哪些项目？").canonical, "Show projects for customer Bosch");
  assert.equal(normalizeQuestion("显示 Customer Bosch 的项目。 ").canonical, "Show projects for customer Bosch");
  assert.equal(normalizeQuestion("项目经理是 Zhang San 的项目有哪些？").canonical, "Show projects managed by Zhang San");
  assert.equal(normalizeQuestion("查找 BM-12345 项目？").canonical, "Show project with MCR-ID BM-12345");
  assert.equal(normalizeQuestion("SE3 超期的项目有哪些？").canonical, "Which projects have overdue SE3");
  assert.equal(normalizeQuestion("哪些项目 QGC4 已经逾期？").canonical, "Which projects have overdue QGC4");
  assert.equal(normalizeQuestion("哪些项目是中风险？").canonical, "Which projects are Medium Risk");
  assert.equal(normalizeQuestion("哪些项目是低风险？").canonical, "Which projects are Low Risk");
  assert.equal(normalizeQuestion("Show projects managed by 张三?").language, "en");
  assert.equal(classifyQuestion("有多少个逾期项目？"), "factual_query");
  assert.equal(classifyQuestion("客户 Bosch 有多少个项目？"), "factual_query");
});

test("Chinese answers use Chinese templates and analytical questions preserve Chinese language", async () => {
  const local = new LocalQueryProvider(
    async () => ({ type: "metric", summary: "3 projects are Critical risk.", value: 3 }),
    async () => "project-list:1:mcr:0",
    async () => ({ type: "metric", summary: "3 projects are Critical risk.", value: 3 })
  );
  const router = new QuestionRouter(local, new LLMProvider(async (_question, language) => ({ text: language === "zh" ? "当前主要风险集中在逾期节点。" : "Overdue milestones are a major risk.", model: "qwen3.6-flash" })), async () => true);
  const answer = await router.askQuestion("有多少个严重风险项目？");
  assert.equal(answer.language, "zh");
  assert.equal(answer.content, "当前共有 3 个 Critical 项目。");
  const analytical = await router.askQuestion("哪些项目最需要关注？为什么？");
  assert.equal(analytical.provider, "llm");
  assert.equal(analytical.language, "zh");
  assert.match(analytical.content, /当前主要风险/);
});

test("Ask Project Data renders and is wired through the router, not the STEP 11 client", () => {
  const markup = renderToStaticMarkup(createElement(MemoryRouter, null, createElement(AskProjectData)));
  assert.match(markup, /Ask Project Data/);
  const source = readFileSync("src/components/insights/AskProjectData.tsx", "utf8");
  assert.match(source, /router\.askQuestion\(text, context\)/);
  assert.doesNotMatch(source, /import\s*\{\s*askProjectData\s*\}/);
});

test("AI answers render readable GFM markdown without executing raw HTML", () => {
  const markdown = "## 风险摘要\n\n**重点项目**\n\n- Alpha\n- Beta\n\n| Owner | Risk |\n| --- | --- |\n| WU Wei | Critical |\n\nUse `risk_level`.\n\n<script>alert('x')</script>";
  const markup = renderToStaticMarkup(createElement(MarkdownAnswer, { content: markdown }));
  assert.match(markup, /<h2>风险摘要<\/h2>/);
  assert.match(markup, /<strong>重点项目<\/strong>/);
  assert.match(markup, /<ul>/);
  assert.match(markup, /ask-markdown-table/);
  assert.match(markup, /<code>risk_level<\/code>/);
  assert.doesNotMatch(markup, /<script>|alert\(&#x27;x&#x27;\)/);
});

test("STEP 13B.2 resolves deterministic and analytical follow-ups from bounded query state", async () => {
  const queries: any[] = [];
  let version = "dataset-v1";
  const local: AIProvider = { async askQuestion(_question, context) {
    const query = context?.structuredQuery!;
    queries.push(query);
    if (query.operation === "count") return { content: "9", answerType: "metric", provider: "local", value: 9, metadata: { datasetVersion: version } };
    if (query.operation === "distinct") return { content: "owners", answerType: "text", provider: "local", items: ["WU Wei"], metadata: { datasetVersion: version } };
    const projects = Array.from({ length: 9 }, (_, index) => ({ id: index + 1, projectName: `Project ${index + 1}`, mcrId: `BM-${index + 1}`, projectStatus: "WORK", riskLevel: "Critical", riskScore: 90 } as any));
    return { content: "projects", answerType: "project_list", provider: "local", total: 9, projects, referencedProjectIds: projects.map((project) => project.id), metadata: { datasetVersion: version } };
  } };
  const llmQueries: any[] = [];
  const llm: AIProvider = { async askQuestion(_question, context) { llmQueries.push(context?.structuredQuery); return { content: "because", answerType: "text", provider: "llm", items: [] }; } };
  const router = new QuestionRouter(local, llm, async () => true, undefined, async () => version);

  await router.askQuestion("Xu Tian 有几个项目？");
  await router.askQuestion("是哪几个？");
  assert.equal(queries.at(-1).operation, "list");
  assert.deepEqual(queries.at(-1).filters, [{ field: "project_manager", operator: "person_name_equals", value: "Xu Tian" }]);

  await router.askQuestion("Customer ABC 有哪些项目？");
  await router.askQuestion("其中哪些是 Critical？");
  assert.equal(queries.at(-1).filters.some((filter: any) => filter.field === "customer" && filter.value === "ABC"), true);
  assert.equal(queries.at(-1).filters.some((filter: any) => filter.field === "risk_level" && filter.value === "Critical"), true);
  assert.deepEqual(queries.at(-1).filters.find((filter: any) => filter.field === "project_id")?.values, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
  await router.askQuestion("其中哪些 SE3 overdue？");
  assert.equal(queries.at(-1).filters.some((filter: any) => filter.field === "milestone" && filter.operator === "overdue" && filter.value === "SE3"), true);

  await router.askQuestion("哪些项目是 Critical？");
  await router.askQuestion("哪些是 WORK 状态？");
  assert.equal(queries.at(-1).filters.some((filter: any) => filter.field === "prj_status" && filter.value === "WORK"), true);
  await router.askQuestion("一共有几个？");
  assert.equal(queries.at(-1).operation, "count");

  await router.askQuestion("Customer ABC 有哪些项目？");
  await router.askQuestion("项目经理分别是谁？");
  assert.equal(queries.at(-1).operation, "distinct");
  assert.equal(queries.at(-1).target, "project_manager");

  await router.askQuestion("哪些项目是 Critical？");
  assert.equal((await router.askQuestion("为什么？")).provider, "llm");
  assert.deepEqual(llmQueries.at(-1).filters.find((filter: any) => filter.field === "project_id")?.values, [1, 2, 3, 4, 5, 6, 7, 8, 9]);

  version = "dataset-v2";
  const stale = await router.askQuestion("其中哪些是 WORK？");
  assert.equal(stale.answerType, "unsupported");
  assert.match(stale.content, /数据已更新/);
  router.resetConversation();
  const cleared = await router.askQuestion("是哪几个？");
  assert.match(cleared.content, /请先说明/);

  const restarted = new QuestionRouter(local, llm, async () => true, undefined, async () => version);
  assert.match((await restarted.askQuestion("是哪几个？")).content, /请先说明/);
});

test("conversation query state is component-local memory and has no persistence layer", () => {
  const routerSource = readFileSync("src/services/ai/QuestionRouter.ts", "utf8");
  const resolverSource = readFileSync("src/services/ai/FollowUpResolver.ts", "utf8");
  const uiSource = readFileSync("src/components/insights/AskProjectData.tsx", "utf8");
  const combined = `${routerSource}\n${resolverSource}\n${uiSource}`;
  assert.match(resolverSource, /lastOperation/);
  assert.match(uiSource, /useState\(\(\) => new AgentQuestionRouter\(\)\)/);
  assert.match(uiSource, /router\.resetConversation\(\)/);
  assert.doesNotMatch(combined, /localStorage|sessionStorage|chat_sessions|chat_messages|saveConversation|restoreConversation/i);
  assert.doesNotMatch(uiSource, /^const router\s*=\s*new QuestionRouter/m);
});

test("STEP 15 uses the LLM agent as the natural-language entry point", async () => {
  const calls: Array<{ question: string; context: any }> = [];
  const llm: AIProvider = { async askQuestion(question, context) {
    calls.push({ question, context });
    return { content: "BM-00108495 的负责人是 Zhang San。", answerType: "text", provider: "llm", language: "zh", items: [] };
  } };
  const fallback: any = { resetConversation() {}, async askQuestion() { throw new Error("deterministic language gate must not run"); } };
  const agent = new AgentQuestionRouter(fallback, llm, async () => true);
  const answer = await agent.askQuestion("BM-00108495 的负责人是谁？", { recentMessages: [] });
  assert.equal(answer.provider, "llm");
  assert.equal(calls[0].question, "BM-00108495 的负责人是谁？");
  assert.equal(calls[0].context.structuredQuery, undefined);
  assert.equal(calls[0].context.requiresCurrentDataset, true);
});

test("STEP 15 keeps deterministic querying only as the offline compatibility path", async () => {
  let fallbackCalls = 0;
  const fallback: any = { resetConversation() {}, async askQuestion() {
    fallbackCalls += 1;
    return { content: "offline", answerType: "text", provider: "local", items: [] };
  } };
  const llm: AIProvider = { async askQuestion() { throw new Error("LLM must not run"); } };
  const agent = new AgentQuestionRouter(fallback, llm, async () => false);
  assert.equal((await agent.askQuestion("How many projects are there?")).provider, "local");
  assert.equal(fallbackCalls, 1);
});

test("Ask Project Data keeps the latest exchange visible without moving keyboard focus", () => {
  const uiSource = readFileSync("src/components/insights/AskProjectData.tsx", "utf8");

  assert.match(uiSource, /conversationRef/);
  assert.match(uiSource, /conversation\.scrollTo\(\{/);
  assert.match(uiSource, /top: conversation\.scrollHeight/);
  assert.match(uiSource, /prefers-reduced-motion: reduce/);
  assert.doesNotMatch(uiSource, /\.focus\(\)/);
});

test("STEP 13D semantic fallback validates plans and keeps factual answers in SQLite", async () => {
  const localQueries: any[] = [];
  const llmContexts: any[] = [];
  const local: AIProvider = { async askQuestion(_question, context) { localQueries.push(context?.structuredQuery); return { content: "WU Wei 当前负责 6 个项目。", answerType: "metric", provider: "local", value: 6, metadata: { datasetVersion: "v1" } }; } };
  const llm: AIProvider = { async askQuestion(_question, context) { llmContexts.push(context); return { content: "Grounded analysis", answerType: "text", provider: "llm", items: [] }; } };
  const managerQuery = { entity: "project", operation: "count", filters: [{ field: "project_manager", operator: "person_name_equals", value: "wu wei" }] } as const;
  const semantic = async (question: string): Promise<any> => question.includes("风险")
    ? { intent: "analytical_question", query: { ...managerQuery, operation: "list", limit: 8 }, confidence: .91 }
    : question.includes("status update")
      ? { intent: "general_conversation", query: null, confidence: .9 }
      : { intent: "factual_query", query: managerQuery, confidence: .94 };
  const router = new QuestionRouter(local, llm, async () => true, undefined, async () => "v1", semantic);

  const factual = await router.askQuestion("wu wei现在手上的项目多不多");
  assert.equal(factual.provider, "local");
  assert.equal((factual as any).value, 6);
  assert.deepEqual(localQueries[0], managerQuery);

  const analytical = await router.askQuestion("帮我看看Xu Tian项目风险");
  assert.equal(analytical.provider, "llm");
  assert.equal(llmContexts.at(-1).structuredQuery.operation, "list");
  assert.equal(llmContexts.at(-1).structuredQuery.limit, 8);

  await router.askQuestion("Customer ABC下面哪些项目值得关注");
  assert.equal(llmContexts.at(-1).structuredQuery.filters.some((filter: any) => filter.field === "customer" && filter.value === "ABC"), true);

  await router.askQuestion("How do I write a concise status update?");
  assert.equal(llmContexts.at(-1).directConversation, true);
});

test("STEP 13D rejects raw SQL and non-whitelisted semantic fields", () => {
  assert.equal(validateSemanticPlan({ intent: "factual_query", query: { entity: "project", operation: "count", filters: [], sql: "SELECT * FROM projects" }, confidence: .99 }), null);
  assert.equal(validateSemanticPlan({ intent: "factual_query", query: { entity: "project", operation: "count", filters: [{ field: "api_key", operator: "equals", value: "x" }] }, confidence: .99 }), null);
  assert.equal(validateSemanticPlan({ intent: "factual_query", query: { entity: "project", operation: "count", filters: [{ field: "project_id", operator: "in", values: [1] }] }, confidence: .99 }), null);
});

test("query completeness gate prevents partial deterministic answers", async () => {
  const explicit = assessStructuredQuery(normalizeQuestion("Customer ABC 有哪些 Critical 项目？"));
  assert.equal(explicit.confidence, "high");
  assert.deepEqual(explicit.query?.filters, [
    { field: "risk_level", operator: "equals", value: "Critical" },
    { field: "customer", operator: "equals", value: "ABC" }
  ]);

  const partialQuestion = "Customer ABC 有哪些项目，最近情况怎么样？";
  const partial = assessStructuredQuery(normalizeQuestion(partialQuestion));
  assert.equal(partial.query?.filters.some((filter) => filter.field === "customer"), true);
  assert.equal(partial.confidence, "low");
  assert.equal(partial.requiresAnalysis, true);

  let localCalled = false;
  let semanticCalled = false;
  const local: AIProvider = { async askQuestion() { localCalled = true; throw new Error("partial query must not execute locally"); } };
  const llm: AIProvider = { async askQuestion() { return { content: "Grounded status analysis", answerType: "text", provider: "llm", items: [] }; } };
  const semantic = async (): Promise<any> => {
    semanticCalled = true;
    return { intent: "analytical_question", query: { entity: "project", operation: "list", filters: [{ field: "customer", operator: "equals", value: "ABC" }], limit: 8 }, confidence: .93 };
  };
  const router = new QuestionRouter(local, llm, async () => true, undefined, async () => "v1", semantic);
  const result = await router.askQuestion(partialQuestion);
  assert.equal(result.provider, "llm");
  assert.equal(semanticCalled, true);
  assert.equal(localCalled, false);
});

test("owner plus risk-detail questions retrieve projects for LLM orchestration instead of returning a raw list", async () => {
  const question = "高风险的项目负责人是谁，有哪些风险";
  const assessment = assessStructuredQuery(normalizeQuestion(question));
  assert.equal(assessment.query?.filters.some((filter) => filter.field === "risk_level" && filter.value === "High"), true);
  assert.equal(assessment.confidence, "low");
  assert.equal(assessment.requiresAnalysis, true);
  assert.equal(assessment.unresolvedTerms.includes("owner and risk details"), true);

  let semanticCalled = false;
  let receivedContext: any;
  const local: AIProvider = { async askQuestion() { throw new Error("multi-part question must not return a raw local project list"); } };
  const llm: AIProvider = { async askQuestion(_question, context) {
    receivedContext = context;
    return { content: "高风险项目由相关负责人管理，主要风险如下。", answerType: "text", provider: "llm", items: [] };
  } };
  const semantic = async (): Promise<any> => {
    semanticCalled = true;
    return {
      intent: "analytical_question",
      query: { entity: "project", operation: "list", filters: [{ field: "risk_level", operator: "equals", value: "High" }], limit: 8 },
      confidence: .96
    };
  };
  const router = new QuestionRouter(local, llm, async () => true, undefined, async () => "v1", semantic);
  const result = await router.askQuestion(question);
  assert.equal(result.provider, "llm");
  assert.equal(semanticCalled, true);
  assert.deepEqual(receivedContext.structuredQuery.filters, [{ field: "risk_level", operator: "equals", value: "High" }]);
});

test("unspecified risk project counts return the deterministic Risk Level distribution", async () => {
  const parsed = parseStructuredQuery(normalizeQuestion("风险项目数量"));
  assert.deepEqual(parsed, {
    entity: "project",
    operation: "group_by",
    target: "risk_level",
    filters: [],
    limit: 4
  });
  assert.deepEqual(classifyQuestion("风险项目数量"), "aggregation");

  let received: unknown;
  const local: AIProvider = { async askQuestion(_question, context) {
    received = context?.structuredQuery;
    return {
      content: "以下是当前数据集的分组统计：",
      answerType: "grouped_summary",
      provider: "local",
      language: "zh",
      groups: [
        { label: "Critical", count: 2 },
        { label: "High", count: 3 },
        { label: "Medium", count: 5 },
        { label: "Low", count: 8 }
      ]
    };
  } };
  const fail: AIProvider = { async askQuestion() { throw new Error("LLM must not be used for the risk distribution"); } };
  const answer = await new QuestionRouter(local, fail, async () => true).askQuestion("风险项目数量");
  assert.equal(answer.answerType, "grouped_summary");
  assert.deepEqual(received, parsed);
});

test("high-risk engineer ranking routes to grounded LLM analysis", () => {
  const router = new QuestionRouter(undefined, undefined, async () => true);
  assert.deepEqual(router.route("给出风险比较大的工程师名字"), {
    category: "analytical_question",
    provider: "llm"
  });
  assert.deepEqual(router.route("Which project managers have the highest risk exposure?"), {
    category: "analytical_question",
    provider: "llm"
  });
});

test("STEP 14 comparison questions use the conversational orchestrator adapter", async () => {
  const local: AIProvider = { async askQuestion() { throw new Error("comparison must not collapse to one local query"); } };
  const seen: AIConversationContext[] = [];
  const llm: AIProvider = { async askQuestion(_question, context) { seen.push(context ?? {}); return { content: "WU Wei has higher current risk exposure.", answerType: "text", provider: "llm", items: [] }; } };
  const router = new QuestionRouter(local, llm, async () => true, undefined, async () => "dataset-v1", async () => { throw new Error("single-query semantic parser must not handle comparisons"); });
  const answer = await router.askQuestion("Compare the projects of WU Wei and Xu Tian", { recentMessages: [{ role: "user", content: "Show their current projects" }] });
  assert.equal(answer.provider, "llm");
  assert.equal(seen[0].structuredQuery, undefined);
  assert.deepEqual(seen[0].recentMessages, [{ role: "user", content: "Show their current projects" }]);
});

test("STEP 14 chat context is bounded in memory and New Chat still clears it", () => {
  const ui = readFileSync("src/components/insights/AskProjectData.tsx", "utf8");
  const client = readFileSync("src/services/ai/enterpriseClient.ts", "utf8");
  assert.match(ui, /exchanges\.slice\(-3\)/);
  assert.match(client, /recentMessages\?\.slice\(-8\)/);
  assert.match(client, /content\.slice\(0, 600\)/);
  assert.match(ui, /router\.resetConversation\(\)/);
  assert.doesNotMatch(`${ui}\n${client}`, /localStorage|sessionStorage|chat_sessions|chat_messages/i);
});

test("STEP 14E prompt evaluation factual cases keep the deterministic SQLite route", () => {
  const cases = JSON.parse(readFileSync("tests/ai-prompt-evaluation.json", "utf8")) as { id: string; question: string; expectedRoute: string }[];
  const router = new QuestionRouter(undefined, undefined, async () => true);
  for (const item of cases.filter((item) => item.expectedRoute === "local_sqlite")) {
    assert.equal(router.route(item.question).provider, "local", item.id);
  }
});
