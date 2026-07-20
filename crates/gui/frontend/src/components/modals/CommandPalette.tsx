import { MagnifyingGlassIcon, MapPinIcon } from "@heroicons/react/24/outline";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAppState, useSimulation } from "../../AppContext";
import { useCanvasSelection } from "../../canvas/selection-context";
import {
  type Command,
  type CommandCategory,
  formatInpImportError,
  openAndLoadNetwork,
  useLinks,
  useNetworkVersion,
  useNodes,
  useProjects,
} from "../../hooks";
import {
  formatPrimaryShortcut,
  formatShortcut,
  primaryModifierLabel,
  shiftModifierLabel,
} from "../../shortcuts";

/**
 * Display-only category union — extends the data-layer `CommandCategory`
 * with a synthetic "Page" group that the palette injects dynamically based
 * on the user's current view. The data layer doesn't know about "Page".
 */
type DisplayCategory = CommandCategory | "Page";

const CATEGORY_ORDER: DisplayCategory[] = [
  "Page",
  "Recent",
  "Navigate",
  "Simulate",
  "Actions",
];

interface DynamicCommand extends Omit<Command, "category"> {
  projectId?: string;
  category: DisplayCategory;
}

interface ElementMatch {
  id: string;
  kind: "node" | "link";
  subtype: string;
  description: string;
}

/** Commands always available regardless of context. */
const STATIC_COMMANDS: DynamicCommand[] = [
  {
    id: "n-settings",
    label: "Settings",
    category: "Navigate",
    action: "nav-settings",
  },
  {
    id: "a-theme-dark",
    label: "Theme: Dark",
    description: "Switch app theme to dark",
    category: "Actions",
    action: "theme-dark",
  },
  {
    id: "a-theme-light",
    label: "Theme: Light",
    description: "Switch app theme to light",
    category: "Actions",
    action: "theme-light",
  },
  {
    id: "a-theme-system",
    label: "Theme: System",
    description: "Follow OS appearance setting",
    category: "Actions",
    action: "theme-system",
  },
];

export function CommandPalette() {
  const {
    closeCommandPalette,
    openProject,
    setPage,
    setProjectView,
    setTheme,
    openRunModal,
    openScenariosModal,
    openIssuesPanel,
    toggleTaskTray,
    showToast,
    page,
    projectView,
    activeProjectId,
    projectsVersion,
  } = useAppState();

  const projects = useProjects(projectsVersion);
  const allNodes = useNodes();
  const allLinks = useLinks();
  const {
    setSelectedNodeId,
    setSelectedLinkId,
    setInspectorView,
    zoomToNode,
    zoomToLink,
  } = useCanvasSelection();
  const { resultMeta } = useSimulation();
  const { bumpNetwork } = useNetworkVersion();

  const allCommands = useMemo<DynamicCommand[]>(
    () => [
      ...projects.slice(0, 5).map<DynamicCommand>((p) => ({
        id: `r-${p.id}`,
        label: p.name,
        description:
          p.state === "simulated"
            ? "Simulated"
            : p.state === "running"
              ? "Running"
              : "Draft",
        category: "Recent",
        action: "open-project",
        projectId: p.id,
      })),
      ...STATIC_COMMANDS,
    ],
    [projects],
  );

  const [query, setQuery] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const modifier = primaryModifierLabel();
  const nav1Shortcut = formatShortcut([modifier, "1"]);
  const nav2Shortcut = formatShortcut([modifier, "2"]);
  const runShortcut = formatPrimaryShortcut("R");
  const toggleLayoutShortcut = formatPrimaryShortcut("M");
  const zoomInShortcut = formatPrimaryShortcut("=");
  const zoomOutShortcut = formatPrimaryShortcut("-");
  const fitShortcut = formatPrimaryShortcut("0");
  const issuesShortcut = formatShortcut([modifier, shiftModifierLabel(), "M"]);

  /** Dynamically computed "Page" group — varies by current page / view. */
  const pageCommands = useMemo<DynamicCommand[]>(() => {
    if (page === "home") {
      return [
        {
          id: "p-new",
          label: "New project",
          description: "Start from a blank network",
          category: "Page",
        },
        {
          id: "p-import",
          label: "Import INP file…",
          description: "Add a new network to a project",
          category: "Page",
        },
        {
          id: "p-projects",
          label: "Browse projects",
          description: "View all saved projects",
          category: "Page",
          action: "nav-projects",
        },
      ];
    }
    if (page === "settings") {
      return [
        {
          id: "p-back-home",
          label: "Back to home",
          category: "Page",
          action: "nav-home",
        },
      ];
    }
    if (page === "project" && activeProjectId) {
      const nav: DynamicCommand[] = [
        {
          id: "n1",
          label: "Canvas",
          category: "Navigate",
          shortcut: nav1Shortcut,
          action: "nav-canvas",
        },
        {
          id: "n2",
          label: "Scenarios",
          category: "Navigate",
          shortcut: nav2Shortcut,
          action: "nav-scenarios",
        },
        {
          id: "n3",
          label: "Analysis",
          category: "Navigate",
          description: "Open the analysis view",
          action: "nav-analysis",
        },
        {
          id: "n4",
          label: "Network Editor",
          category: "Navigate",
          description: "Open the network editor view",
          action: "nav-editor",
        },
      ];
      const simulate: DynamicCommand[] = [
        {
          id: "s1",
          label: "Run simulation",
          description: "Run hydraulics for the active scenario",
          category: "Simulate",
          shortcut: runShortcut,
          action: "run-sim",
        },
      ];
      const actions: DynamicCommand[] = [
        {
          id: "a2",
          label: "Export results to GeoJSON",
          description: "Export node/link results as attributed GeoJSON",
          category: "Actions",
        },
        {
          id: "a4",
          label: "Import INP file…",
          description: "Replace or update the network for this project",
          category: "Actions",
        },
      ];
      const common: DynamicCommand[] = [
        {
          id: "p-run",
          label: "Run simulation",
          description: "Run hydraulics for the active scenario",
          category: "Page",
          shortcut: runShortcut,
          action: "run-sim",
        },
        {
          id: "p-layout-toggle",
          label: "Toggle layout (Geographic/Orthogonal)",
          description: "Switch between geographic and orthogonal layouts",
          category: "Page",
          shortcut: toggleLayoutShortcut,
          action: "canvas-layout-toggle",
        },
        {
          id: "p-layout-map",
          label: "Use geographic layout",
          description: "Switch canvas to geographic map layout",
          category: "Page",
          action: "canvas-layout-map",
        },
        {
          id: "p-layout-schematic",
          label: "Use orthogonal layout",
          description: "Switch canvas to orthogonal schematic layout",
          category: "Page",
          action: "canvas-layout-schematic",
        },
        {
          id: "p-tool-select",
          label: "Use select tool",
          description: "Activate selection tool",
          category: "Page",
          shortcut: "S",
          action: "canvas-tool-select",
        },
        {
          id: "p-tool-edit",
          label: "Use edit tool",
          description: "Activate move/edit nodes tool",
          category: "Page",
          shortcut: "E",
          action: "canvas-tool-edit",
        },
        {
          id: "p-tool-add-node",
          label: "Use add node tool",
          description: "Activate add node tool",
          category: "Page",
          shortcut: "N",
          action: "canvas-tool-add-node",
        },
        {
          id: "p-tool-add-link",
          label: "Use add link tool",
          description: "Activate add link tool",
          category: "Page",
          shortcut: "L",
          action: "canvas-tool-add-link",
        },
        {
          id: "p-tool-measure",
          label: "Use measure tool",
          description: "Activate distance measure tool",
          category: "Page",
          shortcut: "D",
          action: "canvas-tool-measure",
        },
        {
          id: "p-zoom-in",
          label: "Zoom in",
          description: "Zoom the active canvas in",
          category: "Page",
          shortcut: zoomInShortcut,
          action: "canvas-zoom-in",
        },
        {
          id: "p-zoom-out",
          label: "Zoom out",
          description: "Zoom the active canvas out",
          category: "Page",
          shortcut: zoomOutShortcut,
          action: "canvas-zoom-out",
        },
        {
          id: "p-fit-network",
          label: "Fit network",
          description: "Fit the viewport to network bounds",
          category: "Page",
          shortcut: fitShortcut,
          action: "canvas-fit-network",
        },
        {
          id: "p-reset-north",
          label: "Reset north",
          description: "Reset map bearing to north-up",
          category: "Page",
          action: "canvas-reset-north",
        },
        {
          id: "p-issues",
          label: "Open issues panel",
          description: "Review warnings and errors",
          category: "Page",
          shortcut: issuesShortcut,
        },
        {
          id: "p-tasks",
          label: "Open task tray",
          description: "Inspect background runs",
          category: "Page",
        },
      ];
      switch (projectView) {
        case "canvas":
          return [
            ...common,
            {
              id: "p-canvas-find",
              label: "Find element on canvas…",
              description: "Locate a node or link by ID (type # in search)",
              category: "Page",
            },
            ...nav,
            ...simulate,
            ...actions,
          ];
        default:
          return [...common, ...nav, ...simulate, ...actions];
      }
    }
    return [];
  }, [
    page,
    projectView,
    activeProjectId,
    nav1Shortcut,
    nav2Shortcut,
    runShortcut,
    toggleLayoutShortcut,
    zoomInShortcut,
    zoomOutShortcut,
    fitShortcut,
    issuesShortcut,
  ]);

  // "Find element" mode: query starts with `#`. Searches model nodes + links.
  const findMode = query.startsWith("#");
  const findQuery = findMode ? query.slice(1).trim().toLowerCase() : "";

  const elementMatches = useMemo<ElementMatch[]>(() => {
    if (!findMode) return [];
    return [
      ...allNodes
        .filter((n) => n.id.toLowerCase().includes(findQuery))
        .slice(0, 12)
        .map<ElementMatch>((n) => ({
          id: n.id,
          kind: "node",
          subtype: n.type,
          description: `${n.type} · (${n.x}, ${n.y})`,
        })),
      ...allLinks
        .filter((l) => l.id.toLowerCase().includes(findQuery))
        .slice(0, 12)
        .map<ElementMatch>((l) => ({
          id: l.id,
          kind: "link",
          subtype: l.type,
          description: `${l.type} · ${l.fromId} → ${l.toId} · ⌀${l.diameter} mm`,
        })),
    ];
  }, [findMode, findQuery, allNodes, allLinks]);

  // Auto-focus the input when the palette opens.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  // Combined command pool (page-context first, then static commands).
  const ALL_COMMANDS: DynamicCommand[] = useMemo(
    () => [...pageCommands, ...(allCommands as DynamicCommand[])],
    [pageCommands, allCommands],
  );

  // Filtered and grouped results.
  const filtered: DynamicCommand[] = findMode
    ? []
    : query.trim()
      ? ALL_COMMANDS.filter(
          (c) =>
            c.label.toLowerCase().includes(query.toLowerCase()) ||
            c.description?.toLowerCase().includes(query.toLowerCase()),
        )
      : ALL_COMMANDS;

  // Flat ordered list for keyboard navigation.
  const flat: (DynamicCommand | ElementMatch)[] = findMode
    ? elementMatches
    : CATEGORY_ORDER.flatMap((cat) =>
        filtered.filter((c) => c.category === cat),
      );

  // Reset cursor when results change.
  useEffect(() => {
    setActiveIdx(0);
  }, []);

  const execute = useCallback(
    (cmd: DynamicCommand) => {
      // Keep the palette open and switch to # mode so the user can directly
      // type an element id after selecting this helper command.
      if (cmd.id === "p-canvas-find") {
        setProjectView("canvas");
        setQuery("#");
        setActiveIdx(0);
        inputRef.current?.focus();
        return;
      }

      closeCommandPalette();

      // ── Issue / task panel ──────────────────────────────────────────────
      if (cmd.id === "p-issues") {
        openIssuesPanel();
        return;
      }
      if (cmd.id === "p-tasks") {
        toggleTaskTray();
        return;
      }

      // ── Import INP (home or Actions command) ───────────────────────────
      if (cmd.id === "p-import" || cmd.id === "a4") {
        openAndLoadNetwork()
          .then((net) => {
            if (net) {
              bumpNetwork();
              showToast(`Loaded ${net.nodes.length} nodes`, "success");
            }
          })
          .catch((err) => {
            showToast(formatInpImportError(err), "error");
          });
        return;
      }

      // ── New project → navigate home ─────────────────────────────────────
      if (cmd.id === "p-new") {
        setPage("home");
        return;
      }

      // ── Analysis: export GeoJSON ────────────────────────────────────────
      if (cmd.id === "p-an-export" || cmd.id === "a2") {
        if (!resultMeta) {
          showToast("Run a simulation first", "warn");
          return;
        }
        const nodeCoords = new Map(
          allNodes.map((n) => [n.id, [n.x, n.y] as [number, number]]),
        );
        const fc = {
          type: "FeatureCollection" as const,
          features: [
            ...allNodes.map((n) => ({
              type: "Feature" as const,
              geometry: { type: "Point" as const, coordinates: [n.x, n.y] },
              properties: { id: n.id, type: n.type },
            })),
            ...allLinks.map((l) => {
              const from = nodeCoords.get(l.fromId) ?? [0, 0];
              const to = nodeCoords.get(l.toId) ?? [0, 0];
              return {
                type: "Feature" as const,
                geometry: {
                  type: "LineString" as const,
                  coordinates: [from, to],
                },
                properties: { id: l.id, type: l.type },
              };
            }),
          ],
        };
        const blob = new Blob([JSON.stringify(fc, null, 2)], {
          type: "application/json",
        });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = "results.geojson";
        a.click();
        URL.revokeObjectURL(url);
        showToast("Exported results.geojson", "success");
        return;
      }

      // ── Action switch for nav/run commands with explicit action tags ─────
      switch (cmd.action) {
        case "open-project":
          if (cmd.projectId) openProject(cmd.projectId);
          break;
        case "nav-canvas":
          setProjectView("canvas");
          break;
        case "nav-scenarios":
          openScenariosModal();
          break;
        case "nav-analysis":
          setProjectView("analysis");
          break;
        case "nav-editor":
          setProjectView("editor");
          break;
        case "nav-settings":
          setPage("settings");
          break;
        case "nav-home":
          setPage("home");
          break;
        case "nav-projects":
          setPage("projects");
          break;
        case "run-sim":
          openRunModal();
          break;
        case "canvas-layout-toggle":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-layout", { detail: "toggle" }),
          );
          break;
        case "canvas-layout-map":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-layout", { detail: "map" }),
          );
          break;
        case "canvas-layout-schematic":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-layout", { detail: "schematic" }),
          );
          break;
        case "canvas-tool-select":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-tool", { detail: "select" }),
          );
          break;
        case "canvas-tool-edit":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-tool", { detail: "edit" }),
          );
          break;
        case "canvas-tool-add-node":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-tool", { detail: "add-node" }),
          );
          break;
        case "canvas-tool-add-link":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-tool", { detail: "add-link" }),
          );
          break;
        case "canvas-tool-measure":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-tool", { detail: "measure" }),
          );
          break;
        case "canvas-zoom-in":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-viewport", { detail: "zoom-in" }),
          );
          break;
        case "canvas-zoom-out":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-viewport", { detail: "zoom-out" }),
          );
          break;
        case "canvas-fit-network":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-viewport", { detail: "fit" }),
          );
          break;
        case "canvas-reset-north":
          setProjectView("canvas");
          window.dispatchEvent(
            new CustomEvent("hydra:canvas-viewport", {
              detail: "reset-north",
            }),
          );
          break;
        case "theme-dark":
          setTheme("dark");
          break;
        case "theme-light":
          setTheme("light");
          break;
        case "theme-system":
          setTheme("system");
          break;
        default:
          break;
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [
      closeCommandPalette,
      openProject,
      setPage,
      setProjectView,
      setTheme,
      openRunModal,
      openScenariosModal,
      resultMeta,
      allNodes,
      allLinks,
      bumpNetwork,
      openIssuesPanel,
      toggleTaskTray,
      showToast,
    ],
  );

  const executeElement = useCallback(
    (m: ElementMatch) => {
      closeCommandPalette();
      if (page !== "project") {
        showToast("Open a project to navigate to elements", "warn");
        return;
      }

      setProjectView("canvas");
      if (m.kind === "node") {
        setSelectedLinkId(null);
        setSelectedNodeId(m.id);
        setInspectorView("node");
        zoomToNode(m.id);
      } else {
        setSelectedNodeId(null);
        setSelectedLinkId(m.id);
        setInspectorView("link");
        zoomToLink(m.id);
      }
      showToast(`Focused ${m.kind} ${m.id}`, "info");
    },
    [
      closeCommandPalette,
      page,
      setProjectView,
      setSelectedLinkId,
      setSelectedNodeId,
      setInspectorView,
      zoomToNode,
      zoomToLink,
      showToast,
    ],
  );

  // Keyboard navigation.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        closeCommandPalette();
        return;
      }
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setActiveIdx((i) => Math.min(i + 1, flat.length - 1));
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setActiveIdx((i) => Math.max(i - 1, 0));
      }
      if (e.key === "Enter" && flat[activeIdx]) {
        const item = flat[activeIdx];
        if (findMode) executeElement(item as ElementMatch);
        else execute(item as DynamicCommand);
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [activeIdx, closeCommandPalette, execute, executeElement, flat, findMode]);

  // Scroll the active item into view.
  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-idx="${activeIdx}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [activeIdx]);

  // Build grouped view for rendering.
  const groups: { category: DisplayCategory; items: DynamicCommand[] }[] =
    CATEGORY_ORDER.map((cat) => ({
      category: cat,
      items: filtered.filter((c) => c.category === cat),
    })).filter((g) => g.items.length > 0);

  let globalIdx = 0;

  return (
    /* Backdrop */
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the modal on pointer interaction.
    // biome-ignore lint/a11y/useKeyWithClickEvents: backdrop closes the modal on pointer interaction.
    <div
      onClick={closeCommandPalette}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--bg-overlay)",
        zIndex: 200,
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "center",
        paddingTop: 80,
        animation: "fadeIn 120ms ease-out",
      }}
    >
      {/* Panel */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: panel only stops backdrop clicks. */}
      <div
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
        style={{
          width: "100%",
          maxWidth: 560,
          background: "var(--bg-panel)",
          backdropFilter: "blur(24px)",
          border: "1px solid var(--border-hover)",
          borderRadius: 12,
          boxShadow: "var(--shadow-3)",
          animation: "scaleIn 160ms ease-out",
        }}
      >
        {/* Search input */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "12px 16px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <MagnifyingGlassIcon
            style={{
              width: 18,
              height: 18,
              color: "var(--text-tertiary)",
              flexShrink: 0,
            }}
          />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search commands… type # to find elements"
            style={{
              flex: 1,
              border: "none",
              background: "transparent",
              color: "var(--text-primary)",
              fontSize: 14,
              fontFamily: "var(--font-ui)",
              outline: "none",
            }}
          />
          <kbd
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              background: "var(--bg-input)",
              border: "1px solid var(--border-hover)",
              borderRadius: 4,
              padding: "2px 5px",
              fontFamily: "var(--font-mono)",
            }}
          >
            esc
          </kbd>
        </div>

        {/* Results */}
        <div
          ref={listRef}
          style={{ maxHeight: 380, overflowY: "auto", padding: "6px 0" }}
        >
          {flat.length === 0 ? (
            <div
              style={{
                padding: "24px 16px",
                textAlign: "center",
                color: "var(--text-tertiary)",
                fontSize: 13,
              }}
            >
              {findMode ? (
                `No elements match "${findQuery || "…"}"`
              ) : (
                <>No results for &ldquo;{query}&rdquo;</>
              )}
            </div>
          ) : findMode ? (
            <div>
              <div
                style={{
                  padding: "8px 16px 4px",
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                  fontWeight: 600,
                  letterSpacing: "0.06em",
                  textTransform: "uppercase",
                }}
              >
                Find element
              </div>
              {(flat as ElementMatch[]).map((m, i) => {
                const active = i === activeIdx;
                return (
                  <button
                    type="button"
                    key={`${m.kind}-${m.id}`}
                    onClick={() => executeElement(m)}
                    onMouseEnter={() => setActiveIdx(i)}
                    style={{
                      width: "100%",
                      display: "flex",
                      alignItems: "center",
                      gap: 12,
                      padding: "8px 16px",
                      background: active ? "var(--bg-card)" : "transparent",
                      border: "none",
                      cursor: "pointer",
                      textAlign: "left",
                      borderLeft: active
                        ? "2px solid var(--accent)"
                        : "2px solid transparent",
                    }}
                  >
                    <MapPinIcon
                      style={{
                        width: 16,
                        height: 16,
                        flexShrink: 0,
                        color: m.kind === "node" ? "#4a90d9" : "#3daf75",
                      }}
                    />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div
                        style={{
                          fontFamily: "var(--font-mono)",
                          fontSize: 13,
                          color: "var(--text-primary)",
                        }}
                      >
                        {m.id}
                        <span
                          style={{
                            marginLeft: 8,
                            fontSize: 11,
                            color: "var(--text-tertiary)",
                            fontFamily: "var(--font-ui)",
                          }}
                        >
                          {m.kind} · {m.subtype}
                        </span>
                      </div>
                      <div
                        style={{
                          fontSize: 12,
                          color: "var(--text-tertiary)",
                          fontFamily: "var(--font-mono)",
                          marginTop: 1,
                          whiteSpace: "nowrap",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                        }}
                      >
                        {m.description}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          ) : (
            groups.map(({ category, items }) => {
              return (
                <div key={category}>
                  {/* Category header */}
                  <div
                    style={{
                      padding: "6px 16px 2px",
                      fontSize: 11,
                      fontWeight: 600,
                      letterSpacing: "0.07em",
                      textTransform: "uppercase",
                      color: "var(--text-tertiary)",
                    }}
                  >
                    {category}
                  </div>

                  {items.map((cmd) => {
                    const idx = globalIdx++;
                    const isActive = idx === activeIdx;
                    return (
                      <button
                        type="button"
                        key={cmd.id}
                        data-idx={idx}
                        onClick={() => execute(cmd)}
                        onMouseEnter={() => setActiveIdx(idx)}
                        style={{
                          width: "100%",
                          textAlign: "left",
                          border: "none",
                          background: isActive
                            ? "var(--accent-dim)"
                            : "transparent",
                          color: isActive
                            ? "var(--text-primary)"
                            : "var(--text-secondary)",
                          cursor: "pointer",
                          padding: "7px 16px",
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "space-between",
                          gap: 12,
                          fontFamily: "var(--font-ui)",
                          fontSize: 13,
                          transition:
                            "background var(--t-fast), color var(--t-fast)",
                        }}
                      >
                        <div style={{ overflow: "hidden" }}>
                          <div
                            style={{
                              fontWeight: isActive ? 500 : 400,
                              whiteSpace: "nowrap",
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                            }}
                          >
                            {cmd.label}
                          </div>
                          {cmd.description && (
                            <div
                              style={{
                                fontSize: 12,
                                color: "var(--text-tertiary)",
                                marginTop: 1,
                                whiteSpace: "nowrap",
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                              }}
                            >
                              {cmd.description}
                            </div>
                          )}
                        </div>
                        {cmd.shortcut && (
                          <kbd
                            style={{
                              fontSize: 11,
                              color: "var(--text-tertiary)",
                              background: "var(--bg-input)",
                              border: "1px solid var(--border)",
                              borderRadius: 4,
                              padding: "2px 5px",
                              fontFamily: "var(--font-mono)",
                              flexShrink: 0,
                              whiteSpace: "nowrap",
                            }}
                          >
                            {cmd.shortcut}
                          </kbd>
                        )}
                      </button>
                    );
                  })}
                </div>
              );
            })
          )}
        </div>

        {/* Footer hint */}
        <div
          style={{
            padding: "8px 16px",
            borderTop: "1px solid var(--border)",
            display: "flex",
            gap: 16,
            color: "var(--text-tertiary)",
            fontSize: 11,
          }}
        >
          {[
            ["↑↓", "navigate"],
            ["↵", "select"],
            ["esc", "close"],
          ].map(([key, label]) => (
            <span
              key={key}
              style={{ display: "flex", gap: 5, alignItems: "center" }}
            >
              <kbd
                style={{
                  background: "var(--bg-input)",
                  border: "1px solid var(--border)",
                  borderRadius: 3,
                  padding: "1px 4px",
                  fontFamily: "var(--font-mono)",
                  fontSize: 11,
                }}
              >
                {key}
              </kbd>
              {label}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
