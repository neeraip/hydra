import { useEffect, useState } from "react";

/**
 * Live view of an attribute on the document element.
 *
 * The app's display settings — theme, reduce motion, high contrast — are
 * persisted to localStorage and mirrored onto the root element, where the
 * stylesheet reads them. `main.tsx` applies them before first paint, and
 * the Settings drawer rewrites them as they change.
 *
 * The cascade picks that up on its own. Anything painting outside it does
 * not: a canvas draws into a GL context no stylesheet reaches, so it has to
 * read the setting itself. Observing the attribute rather than the stored
 * value means one place is the truth, and a toggle takes effect without a
 * reload however it was made — the drawer, the command palette, or the
 * operating system deciding the theme has changed.
 *
 * Three hooks had a copy of this apiece. They agreed, which is the only
 * reason it never showed.
 */
export function useRootAttribute(name: string): string | null {
  const [value, setValue] = useState<string | null>(() =>
    typeof document === "undefined"
      ? null
      : document.documentElement.getAttribute(name),
  );

  useEffect(() => {
    const root = document.documentElement;
    const read = () => setValue(root.getAttribute(name));
    // Read once on attach: the attribute can have changed between the
    // initial render and this effect, and nothing would report that.
    read();
    const observer = new MutationObserver(read);
    observer.observe(root, { attributes: true, attributeFilter: [name] });
    return () => observer.disconnect();
  }, [name]);

  return value;
}

/** The same, for the settings that are simply on or off. */
export function useRootFlag(name: string): boolean {
  return useRootAttribute(name) === "true";
}
