export type ModelProviderId = "openai-responses";

export interface ModelProviderProfile {
  schemaVersion: string;
  provider: ModelProviderId;
  baseUrl: string;
  model: string;
  credentialRef: string;
}

export interface EnvironmentModelConfiguration {
  apiKeyPresent: boolean;
  modelPresent: boolean;
  model?: string;
  configured: boolean;
}

export interface ModelConfigurationView {
  profile?: ModelProviderProfile;
  credentialConfigured: boolean;
  credentialStoreAvailable: boolean;
  credentialStoreReason?: string;
  environment: EnvironmentModelConfiguration;
}

export interface SaveModelConfigurationInput {
  provider: ModelProviderId;
  baseUrl: string;
  model: string;
  apiKey?: string;
}

export interface ModelConfigurationDraft {
  provider: ModelProviderId;
  baseUrl: string;
  model: string;
  apiKey: string;
}

export interface LiveTestReadiness {
  state: "blocked" | "ready" | "skipped";
  summary: string;
}

export const EMPTY_MODEL_CONFIGURATION: ModelConfigurationView = {
  credentialConfigured: false,
  credentialStoreAvailable: false,
  credentialStoreReason: "浏览器预览无法访问操作系统凭据库",
  environment: {
    apiKeyPresent: false,
    modelPresent: false,
    configured: false,
  },
};

export const DEFAULT_MODEL_DRAFT: ModelConfigurationDraft = {
  provider: "openai-responses",
  baseUrl: "https://api.openai.com/v1",
  model: "",
  apiKey: "",
};

export function draftFromConfiguration(view: ModelConfigurationView): ModelConfigurationDraft {
  return {
    provider: view.profile?.provider ?? "openai-responses",
    baseUrl: view.profile?.baseUrl ?? DEFAULT_MODEL_DRAFT.baseUrl,
    model: view.profile?.model ?? view.environment.model ?? "",
    apiKey: "",
  };
}

export function validateModelConfigurationDraft(draft: ModelConfigurationDraft): string | undefined {
  if (draft.provider !== "openai-responses") return "当前版本只支持 OpenAI Responses provider";
  const model = draft.model.trim();
  if (!model || model.length > 128 || !/^[-A-Za-z0-9._/:]+$/u.test(model)) {
    return "请输入有效的模型 ID";
  }

  let endpoint: URL;
  try {
    endpoint = new URL(draft.baseUrl.trim());
  } catch {
    return "API Base URL 无效";
  }
  if (endpoint.protocol !== "https:" || !endpoint.hostname) return "API Base URL 必须使用 HTTPS";
  if (endpoint.username || endpoint.password || endpoint.search || endpoint.hash) {
    return "API Base URL 不能包含凭据、查询参数或片段";
  }
  const apiKeyCharacters = Array.from(draft.apiKey);
  const hasControlCharacter = apiKeyCharacters.some((character) => {
    const codePoint = character.codePointAt(0) ?? 0;
    return codePoint <= 31 || codePoint === 127;
  });
  const hasNonAsciiCharacter = apiKeyCharacters.some((character) => (character.codePointAt(0) ?? 0) > 127);
  if (draft.apiKey && (draft.apiKey !== draft.apiKey.trim() || draft.apiKey.length < 8 || draft.apiKey.length > 2560 || hasControlCharacter || hasNonAsciiCharacter)) {
    return "API Key 格式无效";
  }
  return undefined;
}

export function liveTestReadiness(view: ModelConfigurationView, runtimeAvailable: boolean): LiveTestReadiness {
  if (!runtimeAvailable) {
    return {
      state: "skipped",
      summary: "Runtime sidecar 不可用，商业 OpenAI 端点 live test 明确未执行。",
    };
  }
  if (!view.environment.configured && !(view.profile && view.credentialConfigured)) {
    return {
      state: "blocked",
      summary: "缺少可用的模型与 API Key，尚不具备连接测试条件。",
    };
  }
  return {
    state: "ready",
    summary: "配置条件已满足；等待 Runtime provider.test 接口执行隔离的连接测试。",
  };
}
