import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

interface TooltipState {
  text: string;
  x: number;
  y: number;
  pos: "top" | "bottom" | "right";
}

const GAP = 8;
const DEFAULT_SHOW_DELAY_MS = 350;

/**
 * Mounts a single global tooltip that listens for mouseenter/mouseleave on
 * any element with a [data-tooltip] attribute. Renders via a React portal
 * directly on <body> so it is never clipped by overflow:hidden ancestors.
 *
 * Position is controlled by [data-tooltip-pos]:
 *   "top"    (default) — above the element, centred
 *   "bottom"           — below the element, centred
 *   "right"            — to the right of the element, vertically centred
 *
 * Appearance delay is controlled by [data-tooltip-delay] in milliseconds.
 * If omitted or invalid, a global default delay is used.
 */
export function TooltipPortal() {
  const [tip, setTip] = useState<TooltipState | null>(null);
  const tipRef = useRef<HTMLDivElement | null>(null);
  const targetRef = useRef<HTMLElement | null>(null);
  const pendingTargetRef = useRef<HTMLElement | null>(null);
  const showTimerRef = useRef<number | null>(null);
  const tipVisibleRef = useRef(false);

  useEffect(() => {
    tipVisibleRef.current = tip !== null;
  }, [tip]);

  useEffect(() => {
    function getPos(el: Element): "top" | "bottom" | "right" {
      const v = (el as HTMLElement).dataset.tooltipPos;
      if (v === "bottom") return "bottom";
      if (v === "right") return "right";
      return "top";
    }

    function getDelay(el: Element): number {
      const raw = (el as HTMLElement).dataset.tooltipDelay;
      if (!raw) return DEFAULT_SHOW_DELAY_MS;
      const n = Number(raw);
      if (!Number.isFinite(n)) return DEFAULT_SHOW_DELAY_MS;
      return Math.max(0, Math.floor(n));
    }

    function clearShowTimer() {
      if (showTimerRef.current != null) {
        window.clearTimeout(showTimerRef.current);
        showTimerRef.current = null;
      }
    }

    function showTip(target: HTMLElement, text: string) {
      targetRef.current = target;
      pendingTargetRef.current = null;

      const rect = target.getBoundingClientRect();
      const pos = getPos(target);

      let x: number;
      let y: number;

      if (pos === "right") {
        x = rect.right + GAP;
        y = rect.top + rect.height / 2;
      } else if (pos === "bottom") {
        x = rect.left + rect.width / 2;
        y = rect.bottom + GAP;
      } else {
        // top
        x = rect.left + rect.width / 2;
        y = rect.top - GAP;
      }

      setTip({ text, x, y, pos });
    }

    function onEnter(e: MouseEvent) {
      const target = (e.target as Element).closest(
        "[data-tooltip]",
      ) as HTMLElement | null;
      if (!target) return;
      const text = target.dataset.tooltip;
      if (!text) return;

      // Ignore repeated enter events for the same anchor while already
      // visible or already queued to show.
      if (targetRef.current === target && tipVisibleRef.current) return;
      if (pendingTargetRef.current === target && !tipVisibleRef.current) return;

      clearShowTimer();
      pendingTargetRef.current = target;

      const delay = getDelay(target);
      if (delay === 0) {
        showTip(target, text);
      } else {
        showTimerRef.current = window.setTimeout(() => {
          showTimerRef.current = null;
          // If the pointer moved elsewhere before the timer fired, abort.
          if (pendingTargetRef.current !== target) return;
          showTip(target, text);
        }, delay);
      }
    }

    function onLeave(e: MouseEvent) {
      const target = (e.target as Element).closest("[data-tooltip]");
      if (!target) return;
      // Only dismiss when the cursor is truly leaving the [data-tooltip] element,
      // not just moving between child elements within it (e.g. SVG paths vs. fill).
      const related = e.relatedTarget as Element | null;
      if (related && target.contains(related)) return;

      // Cancel pending show if we left before the delay elapsed.
      if (
        pendingTargetRef.current &&
        target.contains(pendingTargetRef.current)
      ) {
        pendingTargetRef.current = null;
        clearShowTimer();
      }

      if (targetRef.current && target.contains(targetRef.current)) {
        targetRef.current = null;
        setTip(null);
      }
    }

    // Clicking a tooltip anchor almost always opens something (a popup,
    // dropdown, menu) that the tooltip would otherwise float over or
    // reappear on top of, since the pointer never actually leaves the
    // anchor. Treat mousedown like an immediate leave: hide/cancel the tip.
    // It won't reappear until the pointer truly leaves and re-enters, which
    // naturally covers the entire time the popup/dropdown stays open.
    function onPointerDown() {
      clearShowTimer();
      pendingTargetRef.current = null;
      if (targetRef.current) {
        targetRef.current = null;
        setTip(null);
      }
    }

    // Use capture so we see events on disabled buttons too
    document.addEventListener("mouseenter", onEnter, true);
    document.addEventListener("mouseleave", onLeave, true);
    document.addEventListener("mousedown", onPointerDown, true);
    return () => {
      clearShowTimer();
      document.removeEventListener("mouseenter", onEnter, true);
      document.removeEventListener("mouseleave", onLeave, true);
      document.removeEventListener("mousedown", onPointerDown, true);
    };
  }, []);

  // If the hovered element is removed from the DOM while the tooltip is
  // visible (e.g. a modal closes while the cursor is over a button inside it),
  // the mouseleave event never fires. This MutationObserver watches for DOM
  // removals and dismisses the tip when the anchor element is no longer attached.
  useEffect(() => {
    if (!tip) return;
    const observer = new MutationObserver(() => {
      if (targetRef.current && !document.contains(targetRef.current)) {
        targetRef.current = null;
        pendingTargetRef.current = null;
        if (showTimerRef.current != null) {
          window.clearTimeout(showTimerRef.current);
          showTimerRef.current = null;
        }
        setTip(null);
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [tip]);

  // Runs synchronously after the DOM update but before the browser paints.
  // Computes the final clamped position from the rendered size and applies it
  // directly to the DOM node so there is no visible flash.
  useLayoutEffect(() => {
    const el = tipRef.current;
    if (!el || !tip) return;

    const { width, height } = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;

    let left: number;
    let top: number;

    if (tip.pos === "right") {
      left = tip.x;
      top = tip.y - height / 2;
    } else if (tip.pos === "bottom") {
      left = tip.x - width / 2;
      top = tip.y;
    } else {
      // top
      left = tip.x - width / 2;
      top = tip.y - height;
    }

    // Clamp within viewport with a small margin
    left = Math.max(GAP, Math.min(left, vw - width - GAP));
    top = Math.max(GAP, Math.min(top, vh - height - GAP));

    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  }, [tip]);

  if (!tip) return null;

  // Render offscreen first; useLayoutEffect will clamp before the browser paints.
  const style: React.CSSProperties = {
    position: "fixed",
    zIndex: 99999,
    pointerEvents: "none",
    whiteSpace: "nowrap",
    background: "var(--tooltip-bg, #1e1e2a)",
    color: "var(--tooltip-text, #e2e2ec)",
    fontSize: 12,
    padding: "4px 8px",
    borderRadius: 5,
    border: "1px solid var(--border-hover)",
    boxShadow: "var(--shadow-2)",
    left: -9999,
    top: -9999,
  };

  return createPortal(
    <div ref={tipRef} style={style}>
      {tip.text}
    </div>,
    document.body,
  );
}
