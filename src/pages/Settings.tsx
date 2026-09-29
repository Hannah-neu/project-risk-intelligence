import { isTauri } from "@tauri-apps/api/core";
import { CheckCircle2, FlaskConical, KeyRound, LoaderCircle, LockKeyhole, Save, ShieldCheck } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { PageHeader } from "../components/common/PageHeader";
import { enterpriseErrorMessage, getEnterpriseConfig, saveEnterpriseConfig, testEnterpriseConnection, type EnterpriseConfig, type EnterpriseConfigInput } from "../services/ai/enterpriseClient";

const defaults: EnterpriseConfig = { provider: "local", endpoint: "https://litellm.orchestration.bbm.ai.bosch.com/v1/messages", model: "qwen3.6-flash", maxTokens: 1000, anthropicVersion: "2023-06-01", hasApiKey: false };

export function Settings() {
  const [config, setConfig] = useState<EnterpriseConfig>(defaults);
  const [apiKey, setApiKey] = useState("");
  const [clearApiKey, setClearApiKey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [notice, setNotice] = useState<{ kind: "success" | "error"; text: string } | null>(null);
  const desktop = isTauri();

  useEffect(() => {
    if (!desktop) { setLoading(false); return; }
    getEnterpriseConfig().then(setConfig).catch((error) => setNotice({ kind: "error", text: enterpriseErrorMessage(error) })).finally(() => setLoading(false));
  }, [desktop]);

  function update<K extends keyof EnterpriseConfig>(key: K, value: EnterpriseConfig[K]) {
    setConfig((current) => ({ ...current, [key]: value }));
    setDirty(true);
    setNotice(null);
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setNotice(null);
    const input: EnterpriseConfigInput = { provider: config.provider, endpoint: config.endpoint, model: config.model, maxTokens: config.maxTokens, anthropicVersion: config.anthropicVersion, apiKey: apiKey || undefined, clearApiKey };
    try {
      setConfig(await saveEnterpriseConfig(input));
      setApiKey("");
      setClearApiKey(false);
      setDirty(false);
      setNotice({ kind: "success", text: "AI configuration saved on this Windows account." });
    } catch (error) {
      setNotice({ kind: "error", text: enterpriseErrorMessage(error) });
    } finally { setBusy(false); }
  }

  async function testConnection() {
    setBusy(true);
    setNotice(null);
    try {
      const result = await testEnterpriseConnection();
      setNotice({ kind: "success", text: result.message });
    } catch (error) {
      setNotice({ kind: "error", text: enterpriseErrorMessage(error) });
    } finally { setBusy(false); }
  }

  return <div className="page ai-settings-page">
    <PageHeader eyebrow="AI CONFIGURATION" title="Enterprise AI" description="Choose the provider for analytical questions. Factual project queries remain local." />
    <Link to="/ai-insights" className="ai-settings-back">← Back to AI Insights</Link>
    <div className="ai-settings-grid">
      <section className="card ai-settings-card">
        <div className="ai-settings-heading"><span className="ai-settings-icon"><KeyRound size={23} /></span><div><span className="eyebrow">CONNECTION</span><h2>Provider settings</h2><p>Credentials stay on this device and are never sent to the frontend after saving.</p></div></div>
        {!desktop ? <p className="ai-settings-notice error">Enterprise configuration is available in the desktop application.</p> : loading ? <p className="ai-settings-loading"><LoaderCircle className="spin" size={19} /> Loading configuration…</p> : <form className="ai-settings-form" onSubmit={(event) => void save(event)}>
          <label>Provider<select value={config.provider} onChange={(event) => update("provider", event.target.value as EnterpriseConfig["provider"])}><option value="local">Local only</option><option value="llm">Enterprise LLM</option></select><small>Analytical questions use Enterprise LLM only when selected.</small></label>
          <label>Endpoint<input type="url" value={config.endpoint} onChange={(event) => update("endpoint", event.target.value)} spellCheck={false} required /><small>Restricted to the approved Bosch LiteLLM endpoint.</small></label>
          <div className="ai-settings-row"><label>Model<input value={config.model} onChange={(event) => update("model", event.target.value)} spellCheck={false} required /></label><label>Max Tokens<input type="number" min="1" max="4096" value={config.maxTokens} onChange={(event) => update("maxTokens", Number(event.target.value))} required /></label></div>
          <label>API Key<input type="password" autoComplete="new-password" value={apiKey} onChange={(event) => { setApiKey(event.target.value); setDirty(true); setNotice(null); }} placeholder={config.hasApiKey ? "Saved key • enter to replace" : "Enter Enterprise API key"} /><small>{config.hasApiKey ? "A key is saved securely. Leave blank to keep it." : "No key saved yet."}</small></label>
          {config.hasApiKey && <label className="ai-settings-clear"><input type="checkbox" checked={clearApiKey} onChange={(event) => { setClearApiKey(event.target.checked); setDirty(true); }} /> Remove saved API key</label>}
          {notice && <p role="status" className={`ai-settings-notice ${notice.kind}`}>{notice.kind === "success" ? <CheckCircle2 size={17} /> : <LockKeyhole size={17} />}{notice.text}</p>}
          <div className="ai-settings-actions"><button className="ai-settings-save" type="submit" disabled={busy}><Save size={17} /> Save Settings</button><button className="ai-settings-test" type="button" onClick={() => void testConnection()} disabled={busy || dirty || !config.hasApiKey || config.provider !== "llm"} title={dirty ? "Save changes before testing" : config.provider !== "llm" ? "Select Enterprise LLM and save before testing" : undefined}>{busy ? <LoaderCircle className="spin" size={17} /> : <FlaskConical size={17} />} Test Connection</button></div>
        </form>}
      </section>
      <aside className="card ai-settings-side"><ShieldCheck size={28} /><h2>Privacy boundary</h2><p>Analytical questions send a compact, question-relevant selection of project facts to the Enterprise LLM. The complete SQLite database and Excel files are never sent.</p><div className="ai-settings-boundary"><span>Factual project queries</span><strong>Local SQLite</strong></div><div className="ai-settings-boundary"><span>Analytical questions</span><strong>Selected context</strong></div><div className="ai-settings-boundary"><span>API key</span><strong>Windows-protected</strong></div></aside>
    </div>
  </div>;
}
