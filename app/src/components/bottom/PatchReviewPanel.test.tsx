import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { PatchProposalView } from "../../clients/contracts";
import { PatchReviewPanel } from "./PatchReviewPanel";

const proposal: PatchProposalView = {
  id: "proposal",
  baseRevision: "draft-1",
  status: "ready",
  instruction: "repair route",
  affectedNets: ["net-a"],
  affectedRules: ["clearance"],
  findings: [],
  hunks: [
    { id: "safe", label: "safe hunk", scope: "net", entityIds: ["track-a"], summary: "reroute", status: "pending", protectedImpact: false },
    { id: "locked", label: "locked hunk", scope: "object", entityIds: ["connector"], summary: "preserve", status: "pending", protectedImpact: true },
  ],
};

describe("PatchReviewPanel", () => {
  it("requires every hunk decision and prevents accepting protected impact", () => {
    const apply = vi.fn();
    render(<PatchReviewPanel onApply={apply} onGenerate={vi.fn()} proposal={proposal} />);
    expect(screen.getByRole("button", { name: "接受 locked hunk" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "接受 safe hunk" }));
    fireEvent.click(screen.getByRole("button", { name: "拒绝 locked hunk" }));
    fireEvent.click(screen.getByRole("button", { name: "应用已接受 hunks" }));
    expect(apply).toHaveBeenCalledWith(["safe"], ["locked"]);
  });

  it("blocks direct application of stale proposals", () => {
    render(<PatchReviewPanel onApply={vi.fn()} onGenerate={vi.fn()} proposal={{ ...proposal, status: "stale" }} />);
    expect(screen.getByText(/禁止直接应用/u)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "应用已接受 hunks" })).toBeDisabled();
  });
});
