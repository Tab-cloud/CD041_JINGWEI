import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { approvalFixtures } from "../mocks/v0.1/fixtures";
import { ApprovalDrawer } from "./ApprovalDrawer";

describe("ApprovalDrawer", () => {
  it("shows target, redacted input, effect, scope and expiry", () => {
    render(<ApprovalDrawer approvals={approvalFixtures} onReply={vi.fn()} />);
    expect(screen.getByText("pcb.route_critical")).toBeInTheDocument();
    expect(screen.getByText(/credentials=\[redacted\]/u)).toBeInTheDocument();
    expect(screen.getByText("design-write candidate")).toBeInTheDocument();
    expect(screen.getByText("本次调用、当前项目、选中网络")).toBeInTheDocument();
    expect(screen.getByText(/不能覆盖 daemon deny/u)).toBeInTheDocument();
  });

  it("returns a scoped approve-once decision", () => {
    const onReply = vi.fn();
    render(<ApprovalDrawer approvals={approvalFixtures} onReply={onReply} />);
    fireEvent.click(screen.getByRole("button", { name: "仅批准一次" }));
    expect(onReply).toHaveBeenCalledWith("approval-002", "approve-once");
  });
});
