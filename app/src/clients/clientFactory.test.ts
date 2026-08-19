import { afterEach, describe, expect, it } from "vitest";
import { DesignTransportClient } from "./DesignTransportClient";
import { RuntimeTransportClient } from "./RuntimeTransportClient";
import { createWorkbenchClients } from "./clientFactory";
import { MockDesignClient, MockRuntimeClient } from "../mocks/v0.1";

afterEach(() => window.history.replaceState({}, "", "/"));

describe("workbench transport selection", () => {
  it("does not silently fall back to fixtures in an ordinary browser or production build", () => {
    const clients = createWorkbenchClients();
    expect(clients.mode).toBe("real");
    expect(clients.runtime).toBeInstanceOf(RuntimeTransportClient);
    expect(clients.design).toBeInstanceOf(DesignTransportClient);
  });

  it("enables versioned mocks only through the explicit fixture query", () => {
    window.history.replaceState({}, "", "/?fixture=1");
    const clients = createWorkbenchClients();
    expect(clients.mode).toBe("fixture");
    expect(clients.runtime).toBeInstanceOf(MockRuntimeClient);
    expect(clients.design).toBeInstanceOf(MockDesignClient);
  });
});
