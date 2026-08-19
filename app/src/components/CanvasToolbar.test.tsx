import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CanvasToolbar } from "./CanvasToolbar";

describe("CanvasToolbar accessibility", () => {
  it("exposes pressed tool state and keyboard-operable controls", () => {
    const onToolChange = vi.fn();
    render(<CanvasToolbar onFit={vi.fn()} onToolChange={onToolChange} onZoom={vi.fn()} toolMode="select" view="pcb" zoom={1} />);

    expect(screen.getByRole("toolbar", { name: "画布工具" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "选择" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.keyDown(screen.getByRole("button", { name: "绘制 track" }), { key: "Enter" });
    fireEvent.click(screen.getByRole("button", { name: "绘制 track" }));
    expect(onToolChange).toHaveBeenCalledWith("route");
  });

  it("shows only commands valid for the active view", () => {
    render(<CanvasToolbar onFit={vi.fn()} onToolChange={vi.fn()} onZoom={vi.fn()} toolMode="connect" view="schematic" zoom={0.8} />);

    expect(screen.getByRole("button", { name: "连接 wire" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "放置 via" })).not.toBeInTheDocument();
    expect(screen.getByText("80%")).toBeInTheDocument();
  });
});
