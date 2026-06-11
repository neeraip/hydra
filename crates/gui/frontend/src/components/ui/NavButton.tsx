import type { ReactNode } from "react";

interface NavButtonProps {
  icon: ReactNode;
  label: string;
  active?: boolean;
  onClick?: () => void;
  /** Red badge count — shown when > 0 */
  badgeCount?: number;
  /** Animated blue pulse dot — shown when tasks are running and badgeCount is 0 */
  pulse?: boolean;
  /** Extra class names */
  className?: string;
}

/**
 * Reusable navigation button for the activity bar and secondary rail.
 *
 * Active state is communicated via the CSS `.active` class which renders a
 * 3px left-edge accent bar — NOT a background fill.  All hover behaviour is
 * handled by the `.nav-btn:hover` CSS rule; there are no JS hover handlers.
 */
export function NavButton({
  icon,
  label,
  active = false,
  onClick,
  badgeCount,
  pulse,
  className = "",
}: NavButtonProps) {
  return (
    <button
      className={`nav-btn${active ? " active" : ""}${className ? ` ${className}` : ""}`}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
      data-tooltip={label}
      data-tooltip-pos="right"
    >
      {icon}

      {/* Error badge takes priority over the pulse dot */}
      {badgeCount != null && badgeCount > 0 ? (
        <span className="nav-badge" aria-label={`${badgeCount} failed`}>
          {badgeCount}
        </span>
      ) : pulse ? (
        <span className="nav-pulse-dot" aria-hidden="true" />
      ) : null}
    </button>
  );
}
