import { describe, expect, it } from "vitest";
import {
  DEFAULT_MODEL_DRAFT,
  liveTestReadiness,
  type ModelConfigurationView,
  providerCompatibilityHint,
  validateModelConfigurationDraft,
} from "./modelConfiguration";

const configuredView: ModelConfigurationView = {
  profile: {
    schemaVersion: "0.1",
    provider: "openai-responses",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-example-1",
    credentialRef: "credential://windows/jingwei.openai",
  },
  credentialConfigured: true,
  credentialStoreAvailable: true,
  environment: {
    apiKeyPresent: false,
    modelPresent: false,
    configured: false,
  },
};

describe("model configuration policy", () => {
  it("requires a model and a safe HTTPS endpoint", () => {
    expect(validateModelConfigurationDraft(DEFAULT_MODEL_DRAFT)).toBe("请输入有效的模型 ID");
    expect(validateModelConfigurationDraft({ ...DEFAULT_MODEL_DRAFT, model: "gpt-example", baseUrl: "http://api.openai.com/v1" })).toContain("HTTPS");
    expect(validateModelConfigurationDraft({ ...DEFAULT_MODEL_DRAFT, model: "gpt-example", baseUrl: "https://user:secret@example.com/v1" })).toContain("不能包含凭据");
    expect(validateModelConfigurationDraft({ ...DEFAULT_MODEL_DRAFT, model: "gpt-example" })).toBeUndefined();
  });

  it("rejects unsafe API key shapes before the native bridge", () => {
    expect(validateModelConfigurationDraft({ ...DEFAULT_MODEL_DRAFT, model: "gpt-example", apiKey: " short " })).toContain("API Key");
    expect(validateModelConfigurationDraft({ ...DEFAULT_MODEL_DRAFT, model: "gpt-example", apiKey: "valid-test-key" })).toBeUndefined();
  });

  it("supports explicit Chat Completions and warns about a mismatched MiniMax protocol", () => {
    const minimax = { ...DEFAULT_MODEL_DRAFT, model: "MiniMax-M2.7", baseUrl: "https://api.minimaxi.com/v1" };
    expect(providerCompatibilityHint(minimax)).toContain("Chat Completions");
    const compatible = { ...minimax, provider: "openai-chat-completions" as const };
    expect(validateModelConfigurationDraft(compatible)).toBeUndefined();
    expect(providerCompatibilityHint({ ...compatible, model: "minimax-m3" })).toContain("MiniMax-M3");
    expect(providerCompatibilityHint({ ...compatible, model: "MiniMax-M3" })).toBeUndefined();
  });

  it("reports the commercial endpoint test as skipped when Runtime is unavailable", () => {
    expect(liveTestReadiness(configuredView, false)).toEqual({
      state: "skipped",
      summary: "Runtime sidecar 不可用，商业 OpenAI 端点 live test 明确未执行。",
    });
    expect(liveTestReadiness(configuredView, true).state).toBe("ready");
  });
});
