import { useEffect, useState } from "react";

/**
 * Live view of the app's "Reduce motion" accessibility setting.
 *
 * SettingsPage persists the setting to localStorage and mirrors it onto the
 * root element's `data-reduced-motion` attribute (applied before first paint
 * in main.tsx). Observing the attribute keeps consumers in sync when the
 * setting is toggled without requiring a reload.
 */
export function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () =>
      document.documentElement.getAttribute("data-reduced-motion") === "true",
  );
  useEffect(() => {
    const observer = new MutationObserver(() => {
      setReduced(
        document.documentElement.getAttribute("data-reduced-motion") === "true",
      );
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-reduced-motion"],
    });
    return () => observer.disconnect();
  }, []);
  return reduced;
}
