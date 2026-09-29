import { ArrowUp, Bot, LoaderCircle, RotateCcw, Search, Settings2 } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { RiskBadge, StatusBadge } from "../common/Badge";
import { AgentQuestionRouter } from "../../services/ai/AgentQuestionRouter";
import type { AIAnswer, AIConversationContext } from "../../services/ai/types";

const examples = [
  "How many projects are Critical?",
  "Which projects are overdue?",
  "Which projects contain ECU?",
  "What are the main risk drivers?"
];

interface Exchange { question: string; answer?: AIAnswer; error?: string }

export function AskProjectData() {
  const [router] = useState(() => new AgentQuestionRouter());
  const [question, setQuestion] = useState("");
  const [exchanges, setExchanges] = useState<Exchange[]>([]);
  const [loading, setLoading] = useState(false);
  const conversationRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const conversation = conversationRef.current;
    if (!conversation || exchanges.length === 0) return;

    const frame = window.requestAnimationFrame(() => {
      const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
      conversation.scrollTo({
        top: conversation.scrollHeight,
        behavior: reduceMotion ? "auto" : "smooth",
      });
    });

    return () => window.cancelAnimationFrame(frame);
  }, [exchanges]);

  async function send(value: string) {
    const text = value.trim();
    if (!text || loading) return;
    setQuestion("");
    setLoading(true);
    setExchanges((items) => [...items, { question: text }]);
    try {
      const previous = [...exchanges].reverse().find((item) => item.answer)?.answer;
      const context: AIConversationContext = {
        datasetVersion: typeof previous?.metadata?.datasetVersion === "string" ? previous.metadata.datasetVersion : undefined,
        lastIntent: previous?.answerType,
        lastProjectIds: previous?.referencedProjectIds,
        recentMessages: exchanges.slice(-3).flatMap((item) => item.answer ? [{ role: "user" as const, content: item.question }, { role: "assistant" as const, content: item.answer.content }] : [])
      };
      const answer = await router.askQuestion(text, context);
      setExchanges((items) => items.map((item, index) => index === items.length - 1 ? { ...item, answer } : item));
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setExchanges((items) => items.map((item, index) => index === items.length - 1 ? { ...item, error: message } : item));
    } finally {
      setLoading(false);
    }
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void send(question);
  }

  function newChat() {
    router.resetConversation();
    setExchanges([]);
    setQuestion("");
  }

  return <section className="card ask-project-data" aria-labelledby="ask-project-data-title">
    <div className="ask-header"><span className="ask-icon"><Search size={21} /></span><div><span className="eyebrow">Question routing</span><h2 id="ask-project-data-title">Ask Project Data</h2><p>Factual answers come from your current dataset. Complex questions use a compact, relevant project context.</p></div><div className="ask-header-actions">{exchanges.length > 0 && <button type="button" className="ask-config-link" onClick={newChat} disabled={loading}><RotateCcw size={17} /> New chat</button>}<Link className="ask-config-link" to="/settings"><Settings2 size={17} /> AI Configuration</Link></div></div>
    <div ref={conversationRef} className="ask-conversation" aria-live="polite" aria-relevant="additions text">
      {exchanges.length === 0 && <div className="ask-empty"><Bot size={24} /><strong>Start with a portfolio question</strong><p>Try one of these examples or type your own.</p><div className="ask-examples">{examples.map((example) => <button type="button" key={example} onClick={() => void send(example)} disabled={loading}>{example}</button>)}</div></div>}
      {exchanges.map((exchange, index) => <div className="ask-exchange" key={`${index}-${exchange.question}`}>
        <div className="ask-user-bubble">{exchange.question}</div>
        <div className="ask-answer-bubble"><span className="ask-answer-mark"><Bot size={17} /></span><div className="ask-answer-content">
          {exchange.answer ? <AnswerContent answer={exchange.answer} /> : exchange.error ? <p className="ask-error">{exchange.error}</p> : <p className="ask-thinking"><LoaderCircle size={16} className="spin" /> Checking the current dataset…</p>}
        </div></div>
      </div>)}
    </div>
    <form className="ask-form" onSubmit={submit}><label className="sr-only" htmlFor="ask-project-question">Question</label><input id="ask-project-question" value={question} onChange={(event) => setQuestion(event.target.value)} placeholder="Ask about projects, risks or milestones / 询问项目、风险或里程碑..." maxLength={500} disabled={loading} /><button type="submit" disabled={!question.trim() || loading} aria-label="Send question">{loading ? <LoaderCircle size={18} className="spin" /> : <ArrowUp size={18} />}<span>Send</span></button></form>
  </section>;
}

function AnswerContent({ answer }: { answer: AIAnswer }) {
  return <><MarkdownAnswer content={answer.content} />
    {answer.answerType === "metric" && <strong className="ask-metric">{answer.value}</strong>}
    {answer.answerType === "project_list" && answer.projects.length > 0 && <div className="ask-project-list">{answer.projects.map((project) => <Link to={`/projects/${project.id}`} className="ask-project" key={project.id}><div className="ask-project-main"><strong>{project.projectName}</strong><span>{project.mcrId}</span></div><div className="ask-project-meta"><StatusBadge status={project.projectStatus} /><RiskBadge level={project.riskLevel} /><b>{answer.language === "zh" ? "分数" : "Score"} {project.riskScore}</b></div><small>{project.primaryRiskDriver ?? (answer.language === "zh" ? "未记录显著风险原因" : "No significant risk driver")}</small></Link>)}</div>}
    {answer.answerType === "grouped_summary" && <div className="ask-group-list">{answer.groups.length ? answer.groups.map((group) => <div key={group.label}><span>{group.label}</span><strong>{group.count}</strong></div>) : <p>{answer.language === "zh" ? "未找到符合条件的分组。" : "No matching groups in the current dataset."}</p>}</div>}
    {answer.answerType === "text" && answer.items.length > 0 && <ul className="ask-text-list">{answer.items.map((item) => <li key={item}>{item}</li>)}</ul>}
    {answer.answerType === "unsupported" && <div className="ask-suggestions">Try: {answer.suggestions.join(" · ")}</div>}
  </>;
}

export function MarkdownAnswer({ content }: { content: string }) {
  return <div className="ask-markdown">
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      skipHtml
      components={{
        a: ({ children, ...props }) => <a {...props} target="_blank" rel="noreferrer">{children}</a>,
        table: ({ children, ...props }) => <div className="ask-markdown-table"><table {...props}>{children}</table></div>
      }}
    >{content}</ReactMarkdown>
  </div>;
}
