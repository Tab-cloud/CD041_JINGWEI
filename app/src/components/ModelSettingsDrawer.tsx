import { useEffect, useMemo, useState } from "react";
import {
  DEFAULT_MODEL_DRAFT,
  draftFromConfiguration,
  liveTestReadiness,
  type ModelConfigurationDraft,
  type ModelConfigurationView,
  type ProviderConnectionTestResult,
  validateModelConfigurationDraft,
} from "../model/modelConfiguration";
import {
  deleteModelCredential,
  getModelConfiguration,
  saveModelConfiguration,
  testModelConnection,
} from "../utils/tauriBridge";
import { AlertCircleIcon, CheckCircleIcon, LockIcon, RefreshIcon, XIcon } from "./Icons";

interface ModelSettingsDrawerProps {
  open: boolean;
  runtimeAvailable: boolean;
  onClose: () => void;
}

interface Feedback {
  tone: "danger" | "success" | "warning";
  message: string;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function ModelSettingsDrawer({ open, runtimeAvailable, onClose }: ModelSettingsDrawerProps) {
  const [configuration, setConfiguration] = useState<ModelConfigurationView>();
  const [draft, setDraft] = useState<ModelConfigurationDraft>(DEFAULT_MODEL_DRAFT);
  const [busy, setBusy] = useState<"delete" | "save" | "test">();
  const [feedback, setFeedback] = useState<Feedback>();
  const [testResult, setTestResult] = useState<ProviderConnectionTestResult>();
  const [testCommandError, setTestCommandError] = useState<string>();

  useEffect(() => {
    if (!open) return;
    let active = true;
    void getModelConfiguration()
      .then((view) => {
        if (!active) return;
        setConfiguration(view);
        setDraft(draftFromConfiguration(view));
        setTestResult(undefined);
        setTestCommandError(undefined);
      })
      .catch((error: unknown) => {
        if (active) setFeedback({ tone: "danger", message: errorMessage(error) });
      });
    return () => {
      active = false;
    };
  }, [open]);

  const readiness = useMemo(
    () => configuration ? liveTestReadiness(configuration, runtimeAvailable) : undefined,
    [configuration, runtimeAvailable],
  );
  const hasUnsavedChanges = useMemo(() => {
    if (!configuration) return true;
    const saved = draftFromConfiguration(configuration);
    return Boolean(
      draft.apiKey ||
      draft.provider !== saved.provider ||
      draft.baseUrl.trim().replace(/\/$/u, "") !== saved.baseUrl.replace(/\/$/u, "") ||
      draft.model.trim() !== saved.model,
    );
  }, [configuration, draft]);

  if (!open) return null;

  const close = () => {
    setDraft((current) => ({ ...current, apiKey: "" }));
    onClose();
  };

  const update = (field: keyof ModelConfigurationDraft, value: string) => {
    setDraft((current) => ({ ...current, [field]: value }));
    setTestResult(undefined);
    setTestCommandError(undefined);
  };

  const save = async () => {
    const validationError = validateModelConfigurationDraft(draft);
    if (validationError) {
      setFeedback({ tone: "danger", message: validationError });
      return;
    }
    setBusy("save");
    setFeedback(undefined);
    try {
      const view = await saveModelConfiguration({
        provider: draft.provider,
        baseUrl: draft.baseUrl.trim(),
        model: draft.model.trim(),
        apiKey: draft.apiKey || undefined,
      });
      setConfiguration(view);
      setDraft(draftFromConfiguration(view));
      setTestResult(undefined);
      setTestCommandError(undefined);
      setFeedback({ tone: "success", message: "模型配置已保存；API Key 未返回前端。" });
    } catch (error) {
      setFeedback({ tone: "danger", message: errorMessage(error) });
    } finally {
      setDraft((current) => ({ ...current, apiKey: "" }));
      setBusy(undefined);
    }
  };

  const deleteCredential = async () => {
    setBusy("delete");
    setFeedback(undefined);
    try {
      const view = await deleteModelCredential();
      setConfiguration(view);
      setDraft((current) => ({ ...current, apiKey: "" }));
      setTestResult(undefined);
      setTestCommandError(undefined);
      setFeedback({ tone: "warning", message: "本机 API Key 已从系统凭据库删除。" });
    } catch (error) {
      setFeedback({ tone: "danger", message: errorMessage(error) });
    } finally {
      setBusy(undefined);
    }
  };

  const testConnection = async () => {
    if (hasUnsavedChanges) {
      setFeedback({ tone: "warning", message: "请先保存当前配置，再执行连接测试。" });
      return;
    }
    setBusy("test");
    setFeedback(undefined);
    setTestResult(undefined);
    setTestCommandError(undefined);
    try {
      setTestResult(await testModelConnection());
    } catch (error) {
      setTestCommandError(errorMessage(error));
    } finally {
      setBusy(undefined);
    }
  };

  const testState = testResult?.status ?? (testCommandError ? "failed" : readiness?.state ?? "skipped");
  const testLabel = testResult?.status === "passed"
    ? "PASS · 已连接"
    : testResult?.status === "failed" || testCommandError
      ? "FAILED · 已执行"
      : readiness?.state === "ready"
        ? "READY · 未执行"
        : readiness?.state === "blocked"
          ? "BLOCKED · 未执行"
          : "SKIPPED · 未执行";
  const testSummary = testResult?.status === "passed"
    ? "模型端点已完成一次真实 Responses 请求。"
    : testResult?.error?.summary ?? testCommandError ?? readiness?.summary ?? "正在读取 Runtime 与配置状态。";

  return (
    <div className="model-settings-layer">
      <button aria-label="关闭模型设置" className="model-settings-scrim" onClick={close} type="button" />
      <aside aria-labelledby="model-settings-title" aria-modal="true" className="model-settings-drawer" role="dialog">
        <header className="model-settings-header">
          <div>
            <span className="eyebrow">AGENT PROVIDER</span>
            <h2 id="model-settings-title">模型与 API 凭据</h2>
            <p>配置由精卫 Runtime 使用，密钥只写入操作系统凭据库。</p>
          </div>
          <button aria-label="关闭" className="icon-button" disabled={Boolean(busy)} onClick={close} type="button"><XIcon /></button>
        </header>

        <div className="model-settings-body">
          <section className="configuration-status-section" aria-label="当前配置状态">
            <div className="settings-section-heading">
              <div><span className="eyebrow">CONFIGURATION SOURCES</span><h3>当前配置来源</h3></div>
              {!configuration && !feedback && <RefreshIcon className="settings-spinner" size={16} />}
            </div>
            <div className="configuration-status-grid">
              <div className="configuration-status-card">
                <span>环境变量</span>
                <strong className={configuration?.environment.configured ? "status-good" : "status-warning"}>
                  {configuration?.environment.configured ? "可用" : "缺少配置"}
                </strong>
                <small><code>OPENAI_API_KEY</code> {configuration?.environment.apiKeyPresent ? "已设置" : "未设置"}</small>
                <small><code>JINGWEI_OPENAI_MODEL</code> {configuration?.environment.modelPresent ? `已设置${configuration.environment.model ? ` · ${configuration.environment.model}` : ""}` : "未设置"}</small>
              </div>
              <div className="configuration-status-card">
                <span>桌面应用配置</span>
                <strong className={configuration?.profile && configuration.credentialConfigured ? "status-good" : "status-warning"}>
                  {configuration?.profile && configuration.credentialConfigured ? "可用" : "尚未完成"}
                </strong>
                <small>模型配置 {configuration?.profile ? "已保存" : "未保存"}</small>
                <small>系统凭据 {configuration?.credentialConfigured ? "已保存" : "未保存"}</small>
              </div>
            </div>
            {configuration?.credentialStoreReason && (
              <p className="settings-inline-warning"><AlertCircleIcon size={14} />{configuration.credentialStoreReason}</p>
            )}
          </section>

          <section className="model-provider-form" aria-label="模型配置表单">
            <div className="settings-section-heading"><div><span className="eyebrow">LOCAL PROFILE</span><h3>OpenAI Responses</h3></div></div>
            <label>
              <span>Provider</span>
              <select aria-label="Provider" onChange={(event) => update("provider", event.target.value)} value={draft.provider}>
                <option value="openai-responses">OpenAI Responses API</option>
              </select>
            </label>
            <label>
              <span>API Base URL</span>
              <input aria-label="API Base URL" autoCapitalize="none" onChange={(event) => update("baseUrl", event.target.value)} spellCheck={false} value={draft.baseUrl} />
              <small>仅接受 HTTPS，禁止在 URL 中嵌入用户名、密码或查询参数。</small>
            </label>
            <label>
              <span>模型 ID</span>
              <input aria-label="模型 ID" autoCapitalize="none" onChange={(event) => update("model", event.target.value)} placeholder="输入账号可用的模型 ID" spellCheck={false} value={draft.model} />
            </label>
            <label>
              <span>API Key</span>
              <input
                aria-label="API Key"
                autoComplete="new-password"
                onChange={(event) => update("apiKey", event.target.value)}
                placeholder={configuration?.credentialConfigured ? "已安全保存；留空保持原凭据" : "输入后保存到系统凭据库"}
                spellCheck={false}
                type="password"
                value={draft.apiKey}
              />
              <small>不写入项目、配置 JSON、日志或浏览器存储；保存后不会回显。</small>
            </label>
            <div className="credential-security-note"><LockIcon size={15} /><span>凭据引用：<code>{configuration?.profile?.credentialRef ?? "保存后生成"}</code></span></div>
          </section>

          <section className={`live-test-card state-${testState}`} aria-label="商业端点测试状态" aria-live="polite">
            <div>
              <span className="eyebrow">COMMERCIAL ENDPOINT</span>
              <h3>连接测试状态</h3>
            </div>
            <strong>{busy === "test" ? "TESTING · 请求中" : testLabel}</strong>
            <p>{busy === "test" ? "Runtime 正在验证端点、凭据和 Responses 协议兼容性。" : testSummary}</p>
            {testResult && (
              <dl className="live-test-evidence">
                <div><dt>端点</dt><dd>{testResult.endpointHost}</dd></div>
                <div><dt>模型</dt><dd>{testResult.model}</dd></div>
                <div><dt>耗时</dt><dd>{testResult.latencyMs} ms</dd></div>
                <div><dt>测试时间</dt><dd>{new Date(testResult.testedAt).toLocaleTimeString()}</dd></div>
                {testResult.error?.httpStatus && <div><dt>HTTP</dt><dd>{testResult.error.httpStatus}</dd></div>}
                {testResult.error && <div><dt>错误代码</dt><dd>{testResult.error.code}</dd></div>}
                {testResult.error && <div><dt>可重试</dt><dd>{testResult.error.retryable ? "是" : "否"}</dd></div>}
              </dl>
            )}
            <div className="live-test-actions">
              <small>
                {hasUnsavedChanges
                  ? "当前表单有未保存修改；测试只使用已保存配置。"
                  : "API Key 由桌面端安全读取；前端只接收脱敏测试证据。"}
              </small>
              <button
                className="secondary-button connection-test-button"
                disabled={readiness?.state !== "ready" || hasUnsavedChanges || Boolean(busy)}
                onClick={() => void testConnection()}
                type="button"
              >
                {busy === "test" ? <RefreshIcon className="settings-spinner" size={14} /> : null}
                {busy === "test" ? "测试中…" : testResult || testCommandError ? "重新测试" : "测试连接"}
              </button>
            </div>
          </section>

          {feedback && (
            <p className={`settings-feedback tone-${feedback.tone}`} role={feedback.tone === "danger" ? "alert" : "status"}>
              {feedback.tone === "success" ? <CheckCircleIcon size={15} /> : <AlertCircleIcon size={15} />}{feedback.message}
            </p>
          )}
        </div>

        <footer className="model-settings-footer">
          <button className="secondary-button danger" disabled={!configuration?.credentialConfigured || Boolean(busy)} onClick={() => void deleteCredential()} type="button">删除本机凭据</button>
          <span />
          <button className="secondary-button" disabled={Boolean(busy)} onClick={close} type="button">取消</button>
          <button className="primary-button" disabled={!configuration?.credentialStoreAvailable || Boolean(busy)} onClick={() => void save()} type="button">
            {busy === "save" ? "保存中…" : "保存配置"}
          </button>
        </footer>
      </aside>
    </div>
  );
}
