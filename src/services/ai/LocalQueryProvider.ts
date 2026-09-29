import { askProjectData, askStructuredProjectData, getDatasetVersion } from "../database/databaseClient";
import type { QaAnswer } from "../../types/qa";
import type { AIProvider } from "./AIProvider";
import { formatLocalAnswer, formatLocalItems } from "./LocalAnswerFormatter";
import type { AIAnswer, AIConversationContext } from "./types";

type Query = (question: string) => Promise<QaAnswer>;
type VersionReader = () => Promise<string>;

export class LocalQueryProvider implements AIProvider {
  constructor(
    private readonly query: Query = askProjectData,
    private readonly readVersion: VersionReader = getDatasetVersion,
    private readonly structuredQuery: (query: NonNullable<AIConversationContext["structuredQuery"]>) => Promise<QaAnswer> = askStructuredProjectData
  ) {}

  async askQuestion(question: string, context?: AIConversationContext): Promise<AIAnswer> {
    const result = context?.structuredQuery ? await this.structuredQuery(context.structuredQuery) : await this.query(question);
    const datasetVersion = await this.readVersion();
    const metadata: Record<string, unknown> = { datasetVersion };
    const language = context?.language ?? "en";
    const content = formatLocalAnswer(result, question, language, context?.structuredQuery);

    switch (result.type) {
      case "metric":
        return { content, answerType: "metric", provider: "local", language, value: result.value, metadata };
      case "project_list":
        return {
          content, answerType: "project_list", provider: "local", language,
          total: result.total, projects: result.projects,
          referencedProjectIds: result.projects.map((project) => project.id), metadata
        };
      case "grouped_summary":
        return { content, answerType: "grouped_summary", provider: "local", language, groups: result.groups, metadata };
      case "text_summary":
        return { content, answerType: "text", provider: "local", language, items: formatLocalItems(result.items, language), metadata };
      case "unsupported_question":
        return { content, answerType: "unsupported", provider: "local", language, suggestions: language === "zh" ? ["有多少个严重风险项目？", "哪些项目逾期了？", "显示客户 X 的项目。"] : result.suggestions, metadata };
    }
  }
}
