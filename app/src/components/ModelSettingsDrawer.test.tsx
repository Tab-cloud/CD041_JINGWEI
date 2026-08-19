import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ModelConfigurationView } from "../model/modelConfiguration";
import { ModelSettingsDrawer } from "./ModelSettingsDrawer";

const bridge = vi.hoisted(() => ({
  deleteModelCredential: vi.fn(),
  getModelConfiguration: vi.fn(),
  saveModelConfiguration: vi.fn(),
}));

vi.mock("../utils/tauriBridge", () => bridge);

const emptyView: ModelConfigurationView = {
  credentialConfigured: false,
  credentialStoreAvailable: true,
  environment: {
    apiKeyPresent: false,
    modelPresent: false,
    configured: false,
  },
};

const savedView: ModelConfigurationView = {
  ...emptyView,
  credentialConfigured: true,
  profile: {
    schemaVersion: "0.1",
    provider: "openai-responses",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-example-1",
    credentialRef: "credential://windows/jingwei.openai",
  },
};

describe("ModelSettingsDrawer", () => {
  beforeEach(() => {
    bridge.getModelConfiguration.mockReset().mockResolvedValue(emptyView);
    bridge.saveModelConfiguration.mockReset().mockResolvedValue(savedView);
    bridge.deleteModelCredential.mockReset().mockResolvedValue(emptyView);
  });

  it("shows missing environment values and the explicitly skipped live test", async () => {
    render(<ModelSettingsDrawer onClose={vi.fn()} open runtimeAvailable={false} />);
    expect(await screen.findByText("SKIPPED · 未执行")).toBeInTheDocument();
    expect(screen.getByText(/OPENAI_API_KEY/).closest("small")).toHaveTextContent("未设置");
    expect(screen.getByText(/JINGWEI_OPENAI_MODEL/).closest("small")).toHaveTextContent("未设置");
    expect(screen.getByText(/商业 OpenAI 端点 live test 明确未执行/)).toBeInTheDocument();
  });

  it("sends a new key once and clears it after the native save", async () => {
    render(<ModelSettingsDrawer onClose={vi.fn()} open runtimeAvailable={false} />);
    await screen.findByText("SKIPPED · 未执行");
    fireEvent.change(screen.getByLabelText("模型 ID"), { target: { value: "gpt-example-1" } });
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "secret-test-key" } });
    fireEvent.click(screen.getByRole("button", { name: "保存配置" }));

    await waitFor(() => expect(bridge.saveModelConfiguration).toHaveBeenCalledWith({
      provider: "openai-responses",
      baseUrl: "https://api.openai.com/v1",
      model: "gpt-example-1",
      apiKey: "secret-test-key",
    }));
    await waitFor(() => expect(screen.getByLabelText("API Key")).toHaveValue(""));
    expect(screen.getByText("模型配置已保存；API Key 未返回前端。")).toBeInTheDocument();
  });
});
