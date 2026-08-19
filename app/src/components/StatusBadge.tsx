import type { ReactNode } from "react";

interface StatusBadgeProps {
  tone: "neutral" | "info" | "success" | "warning" | "danger" | "mock";
  children: ReactNode;
  title?: string;
}

export function StatusBadge({ tone, children, title }: StatusBadgeProps) {
  return <span className={`status-badge tone-${tone}`} title={title}>{children}</span>;
}
