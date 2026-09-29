import { explainSystemQuestion } from "../database/databaseClient";
import type { QuestionLanguage } from "./QuestionNormalizer";
import type { SystemQuestionIntent } from "./SystemQuestionClassifier";
import type { AIAnswer } from "./types";

export class SystemExplanationProvider {
  constructor(private readonly explain = explainSystemQuestion) {}

  async answer(intent: SystemQuestionIntent, question: string, language: QuestionLanguage): Promise<AIAnswer> {
    const content = await this.explain(intent, question, language);
    return { content, answerType: "text", items: [], provider: "local", language, metadata: { intent } };
  }
}
