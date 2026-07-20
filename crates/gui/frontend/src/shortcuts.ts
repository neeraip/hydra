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

export function altModifierLabel(): string {
  return isMacLikePlatform() ? "⌥" : "Alt";
}

export function primaryModifierPressed(
  e: Pick<KeyboardEvent, "metaKey" | "ctrlKey">,
): boolean {
  return isMacLikePlatform() ? e.metaKey : e.ctrlKey;
}

export function formatShortcut(parts: string[]): string {
  return isMacLikePlatform() ? parts.join("") : parts.join("+");
}

export function formatPrimaryShortcut(key: string): string {
  return formatShortcut([primaryModifierLabel(), key]);
}
