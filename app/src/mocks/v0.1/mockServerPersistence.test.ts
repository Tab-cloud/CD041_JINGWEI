import { afterEach, describe, expect, it } from "vitest";
import { loadMockServerState, saveMockServerState } from "./mockServerPersistence";

afterEach(() => {
  sessionStorage.clear();
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("versioned mock server persistence", () => {
  it("does not persist server state in a browser preview", () => {
    saveMockServerState("design", { revision: "rev-draft-012" });
    expect(loadMockServerState("design")).toBeUndefined();
  });

  it("restores Tauri mock state across WebView reloads", () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    saveMockServerState("runtime", { schemaVersion: "0.1", lastSequence: 6 });
    expect(loadMockServerState("runtime")).toEqual({ schemaVersion: "0.1", lastSequence: 6 });
  });
});
