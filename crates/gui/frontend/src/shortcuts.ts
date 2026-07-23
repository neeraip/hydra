export function isMacLikePlatform(): boolean {
  if (typeof navigator === "undefined") return false;

  const nav = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };

  const platform = (
    nav.userAgentData?.platform ??
    navigator.platform ??
    ""
  ).toLowerCase();

  return (
    platform.includes("mac") ||
    platform.includes("iphone") ||
    platform.includes("ipad") ||
    platform.includes("ipod")
  );
}

export function primaryModifierLabel(): string {
  return isMacLikePlatform() ? "⌘" : "Ctrl";
}

export function shiftModifierLabel(): string {
  return isMacLikePlatform() ? "⇧" : "Shift";
}

export function primaryModifierPressed(
  e: Pick<KeyboardEvent, "metaKey" | "ctrlKey">,
): boolean {
  return isMacLikePlatform() ? e.metaKey : e.ctrlKey;
}

export function formatShortcut(parts: string[]): string {
  return isMacLikePlatform() ? parts.join("") : parts.join("+");
}

/** DOM id of the Projects page search input, focused by ⌘F / Ctrl-F.
 *  Lives here (not in ProjectsPage) so App.tsx can reference it without
 *  statically importing the lazily-loaded ProjectsPage chunk. */
export const PROJECTS_SEARCH_INPUT_ID = "projects-search-input";

/**
 * True when a keyboard event originated inside a text-entry control
 * (input/textarea/select/contentEditable). Single-key shortcuts (like `?`)
 * must not fire while the user is typing.
 */
export function isEditableEventTarget(target: EventTarget | null): boolean {
  if (typeof HTMLElement === "undefined" || !(target instanceof HTMLElement)) {
    return false;
  }
  if (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  ) {
    return true;
  }
  return target.isContentEditable;
}

export function formatPrimaryShortcut(key: string): string {
  return formatShortcut([primaryModifierLabel(), key]);
}
