import {
  ExclamationTriangleIcon,
  InformationCircleIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import type { ReactNode } from "react";

interface WarningRowProps {
  children: React.ReactNode;
  /** "warning" (default amber), "error" (red), "info" (accent) */
  level?: "warning" | "error" | "info";
}

const ICON: Record<NonNullable<WarningRowProps["level"]>, ReactNode> = {
  warning: <ExclamationTriangleIcon style={{ width: 14, height: 14 }} />,
  error: <XMarkIcon style={{ width: 14, height: 14 }} />,
  info: <InformationCircleIcon style={{ width: 14, height: 14 }} />,
};

const COLOR: Record<NonNullable<WarningRowProps["level"]>, string> = {
  warning: "var(--status-warning)",
  error: "var(--status-error)",
  info: "var(--accent)",
};

export function WarningRow({ children, level = "warning" }: WarningRowProps) {
  return (
    <div className="warning-row">
      <span style={{ color: COLOR[level], flexShrink: 0 }}>{ICON[level]}</span>
      <span>{children}</span>
    </div>
  );
}
