import { afterEach, describe, expect, it, vi } from "vitest";
import {
  deleteModelCredential,
  getModelConfiguration,
  getSidecarStatuses,
  openExternalUrl,
  saveModelConfiguration,
  testModelConnection,
  validateCredentialReference,
} from "./tauriBridge";

describe("browser security bridge", () => {
  afterEach(() => vi.restoreAllMocks());

  it("reports unavailable sidecars without claiming a real connection", async () => {
    const statuses = await getSidecarStatuses();
    expect(statuses.every((status) => status.state === "unavailable")).toBe(true);
  });

  it("rejects unsafe external links and embedded credentials", async () => {
    await expect(openExternalUrl("javascript:alert(1)")).rejects.toThrow();
    await expect(openExternalUrl("https://user:secret@example.com")).rejects.toThrow();
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    await openExternalUrl("https://example.com/evidence");
    expect(open).toHaveBeenCalledWith("https://example.com/evidence", "_blank", "noopener,noreferrer");
  });

  it("accepts credential references but rejects secret-shaped values", async () => {
    await expect(validateCredentialReference("credential://windows/jingwei.openai")).resolves.toBe("credential://windows/jingwei.openai");
    await expect(validateCredentialReference("credential://macos/jingwei.openai")).resolves.toBe("credential://macos/jingwei.openai");
    await expect(validateCredentialReference("sk-secret")).rejects.toThrow();
  });

  it("does not emulate credential writes in the browser preview", async () => {
    const view = await getModelConfiguration();
    expect(view.credentialStoreAvailable).toBe(false);
    expect(view.environment.configured).toBe(false);
    await expect(saveModelConfiguration({ provider: "openai-responses", baseUrl: "https://api.openai.com/v1", model: "gpt-example" })).rejects.toThrow("原生桌面应用");
    await expect(deleteModelCredential()).rejects.toThrow("原生桌面应用");
    await expect(testModelConnection()).rejects.toThrow("原生桌面应用");
  });
});
