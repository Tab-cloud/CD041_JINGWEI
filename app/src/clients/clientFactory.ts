import type { DesignClient, RuntimeClient } from "./contracts";
import { DesignTransportClient } from "./DesignTransportClient";
import { RuntimeTransportClient } from "./RuntimeTransportClient";
import { MockDesignClient, MockRuntimeClient } from "../mocks/v0.1";
import { isTauriRuntime } from "../utils/tauriBridge";

export interface WorkbenchClients {
  runtime: RuntimeClient;
  design: DesignClient;
  mode: "real" | "fixture";
}

export function createWorkbenchClients(): WorkbenchClients {
  if (isTauriRuntime()) {
    return { runtime: new RuntimeTransportClient(), design: new DesignTransportClient(), mode: "real" };
  }
  if (fixtureModeRequested()) {
    return { runtime: new MockRuntimeClient("normal"), design: new MockDesignClient("normal"), mode: "fixture" };
  }
  return { runtime: new RuntimeTransportClient(), design: new DesignTransportClient(), mode: "real" };
}

function fixtureModeRequested(): boolean {
  return new URLSearchParams(window.location.search).get("fixture") === "1";
}
