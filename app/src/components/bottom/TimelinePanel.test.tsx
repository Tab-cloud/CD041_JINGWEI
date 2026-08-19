import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RunView } from "../../clients/contracts";
import { TimelinePanel } from "./TimelinePanel";

describe("TimelinePanel", () => {
  it("renders structured failures instead of an indefinite spinner", () => {
    const run: RunView = {
      id: "run-failed",
      sessionId: "session",
      title: "failed run",
      status: "failed",
      timeline: [{
        id: "tool",
        kind: "tool",
        title: "design.commit",
        detail: "stale base",
        status: "failed",
        startedAt: "now",
        error: { code: "STALE_BASE_REVISION", summary: "draft changed", suggestedAction: "reload and preview" },
      }],
    };
    render(<TimelinePanel run={run} />);
    expect(screen.getByText("STALE_BASE_REVISION")).toBeInTheDocument();
    expect(screen.getByText("reload and preview")).toBeInTheDocument();
    expect(screen.getByText("failed", { selector: ".run-state" })).toBeInTheDocument();
  });
});
