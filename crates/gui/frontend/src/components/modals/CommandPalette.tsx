import { MagnifyingGlassIcon, MapPinIcon } from "@heroicons/react/24/outline";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useAppState, useSimulation } from "../../AppContext";
import { useCanvasSelection } from "../../canvas/selection-context";
import { engineComponents } from "../../engine/registry";
import {
  type Command,
  type CommandCategory,
  formatInpImportError,
  type Link,
  type Node,
  openAndLoadNetwork,
  useLinks,
  useNetworkVersion,
  useNodes,
  useProjects,
  useRegions,
  useScenarios,
} from "../../hooks";
import { tryInvoke } from "../../hooks/ipc";
import {
  formatPrimaryShortcut,
  formatShortcut,
  primaryModifierLabel,
  shiftModifierLabel,
} from "../../shortcuts";
import { formatQtyRaw, type UnitSystem, useUnitSystem } from "../../units";
import { lineageLabel } from "../panels/ScenariosPanel/shared";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

/**
 * Display-only category union — extends the data-layer `CommandCategory`
 * with a synthetic "Page" group that the palette injects dynamically based
 * on the user's current view. The data layer doesn't know about "Page".
 */
type DisplayCategory = CommandCategory | "Page" | "Scenarios";

const CATEGORY_ORDER: DisplayCategory[] = [
  "Page",
  "Recent",
  "Navigate",
  "Simulate",
  "Scenarios",
  "Actions",
];

interface DynamicCommand extends Omit<Command, "category"> {
  projectId?: string;
  /** Target for the "switch-scenario" action. */
  scenarioId?: string;
  category: DisplayCategory;
}

export interface ElementMatch {
  id: string;
  kind: "node" | "link";
  subtype: string;
  description: string;
}

/** Maximum matches returned per element kind in find-element mode. */
export const FIND_MAX_PER_KIND = 12;

/**
 * Find-element search: case-insensitive substring match of `findQuery`
 * (expected pre-lowercased and trimmed by the caller) against node/link ids.
 * Early-exit loops instead of full `.filter()` passes: with ~46k nodes/links
 * a full scan per keystroke is wasted work once the first `maxPerKind`
 * matches per kind are found. Nodes are listed before links.
 */
export function searchElements(
  allNodes: readonly Node[],
  allLinks: readonly Link[],
  findQuery: string,
  maxPerKind: number = FIND_MAX_PER_KIND,
  sys: UnitSystem = "si",
): ElementMatch[] {
  const matches: ElementMatch[] = [];
  let found = 0;
  for (const n of allNodes) {
    if (found >= maxPerKind) break;
    if (!n.id.toLowerCase().includes(findQuery)) continue;
    matches.push({
      id: n.id,
      kind: "node",
      subtype: n.type,
      description: `${n.type} · (${n.x}, ${n.y})`,
    });
    found += 1;
  }
  found = 0;
  for (const l of allLinks) {
    if (found >= maxPerKind) break;
    if (!l.id.toLowerCase().includes(findQuery)) continue;
    matches.push({
      id: l.id,
      kind: "link",
      subtype: l.type,
      // Diameter only when the engine served one — "⌀0 m" on every
      // attribute-less link read as data.
      description: `${l.type} · ${l.fromId} → ${l.toId}${
        l.diameter != null && l.diameter > 0
          ? ` · ⌀${formatQtyRaw(l.diameter, "diameter", sys)}`
          : ""
      }`,
    });
    found += 1;
  }
  return matches;
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
  {
    id: "a-docs",
    label: "Open documentation",
    description: "Open the Hydra docs in your browser",
    category: "Actions",
  },
];

/** Hydra documentation site — opened by the "Open documentation" command. */
const DOCS_URL = "https://neeraip.github.io/hydra/";

export function CommandPalette() {
  const sys = useUnitSystem();
  const {
    closeCommandPalette,
    openProject,
    closeProject,
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
    activeScenarioId,
    setActiveScenarioId,
    projectsVersion,
    scenariosVersion,
    requestClearResults,
  } = useAppState();

  const projects = useProjects(projectsVersion);
  // Engine key of the open project — the "import model file" action needs it
  // to pick the right file filter and parser.
  const activeProjectEngine =
    projects.find((p) => p.id === activeProjectId)?.engine ?? null;
  // Same gate as the toolbar and shortcuts: editing tool commands do not
  // exist for read-only engines — the palette must not bypass the registry.
  const modelEditable = engineComponents(activeProjectEngine).modelEditable;
  // Scenario quick-switch entries — only meaningful with a project open.
  const scenarios = useScenarios(
    page === "project" ? activeProjectId : null,
    scenariosVersion,
  );
  const allNodes = useNodes();
  const allLinks = useLinks();
  const allRegions = useRegions();
  const {
    setSelectedNodeId,
    setSelectedLinkId,
    setInspectorView,
    zoomToNode,
    zoomToLink,
  } = useCanvasSelection();
  const { resultMeta } = useSimulation();
  const { bumpNetwork } = useNetworkVersion();

  const [query, setQuery] = useState("");
  // Recent projects when idle; all projects become searchable once the user
  // types, so quick-open reaches any project by name (not just the last 5).
  const allCommands = useMemo<DynamicCommand[]>(
    () => [
      ...projects
        .slice(0, query.trim() ? projects.length : 5)
        .map<DynamicCommand>((p) => ({
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
    [projects, query],
  );
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const modifier = primaryModifierLabel();
  const navCanvasShortcut = formatShortcut([modifier, "2"]);
  const navEditorShortcut = formatShortcut([modifier, "3"]);
  const navAnalysisShortcut = formatShortcut([modifier, "4"]);
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
          shortcut: navCanvasShortcut,
          action: "nav-canvas",
        },
        {
          id: "n2",
          label: "Scenarios",
          category: "Navigate",
          action: "nav-scenarios",
        },
        {
          id: "n3",
          label: "Analysis",
          category: "Navigate",
          description: "Open the analysis view",
          shortcut: navAnalysisShortcut,
          action: "nav-analysis",
        },
        {
          id: "n4",
          label: "Network Editor",
          category: "Navigate",
          description: "Open the network editor view",
          shortcut: navEditorShortcut,
          action: "nav-editor",
        },
      ];
      const simulate: DynamicCommand[] = [
        {
          id: "s1",
          label: "Run simulation",
          description: "Run a simulation of the active scenario",
          category: "Simulate",
          shortcut: runShortcut,
          action: "run-sim",
        },
      ];
      const actions: DynamicCommand[] = [
        {
          id: "a2",
          label: "Export results to GeoJSON",
          description:
            "Export nodes/links with attributes and result values when available",
          category: "Actions",
        },
        {
          id: "a-export-inp",
          label: "Export INP…",
          description: "Save the current network as a model input file",
          category: "Actions",
        },
        // Only listed when simulation results exist for the active scenario.
        ...(resultMeta
          ? [
              {
                id: "a-export-csv",
                label: "Export results as CSV…",
                description: "Save the simulation results as a CSV file",
                category: "Actions",
              } satisfies DynamicCommand,
              {
                id: "a-clear-results",
                label: "Clear simulation results",
                description:
                  "Return the active scenario to an unsimulated state",
                category: "Actions",
              } satisfies DynamicCommand,
              {
                id: "a-clear-all-results",
                label: "Clear all simulation results",
                description:
                  "Return the base model and every scenario to an unsimulated state",
                category: "Actions",
              } satisfies DynamicCommand,
            ]
          : []),
        {
          id: "a4",
          label: "Import model file…",
          description: "Replace or update the network for this project",
          category: "Actions",
        },
      ];
      const common: DynamicCommand[] = [
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
        // Editing tools exist only for engines whose model this GUI edits
        // (the CanvasView event listener re-checks, but a listed command
        // that no-ops would read as broken).
        ...(modelEditable
          ? [
              {
                id: "p-tool-edit",
                label: "Use edit tool",
                description: "Activate move/edit nodes tool",
                category: "Page",
                shortcut: "E",
                action: "canvas-tool-edit",
              } satisfies DynamicCommand,
              {
                id: "p-tool-add-node",
                label: "Use add node tool",
                description: "Activate add node tool",
                category: "Page",
                shortcut: "N",
                action: "canvas-tool-add-node",
              } satisfies DynamicCommand,
              {
                id: "p-tool-add-link",
                label: "Use add link tool",
                description: "Activate add link tool",
                category: "Page",
                shortcut: "L",
                action: "canvas-tool-add-link",
              } satisfies DynamicCommand,
            ]
          : []),
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
    resultMeta,
    modelEditable,
    navCanvasShortcut,
    navEditorShortcut,
    navAnalysisShortcut,
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

  const elementMatches = useMemo<ElementMatch[]>(
    () =>
      findMode
        ? searchElements(allNodes, allLinks, findQuery, undefined, sys)
        : [],
    [findMode, findQuery, allNodes, allLinks, sys],
  );

  // Auto-focus the input when the palette opens.
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  /** One "switch scenario" entry per scenario, breadcrumbed by lineage. */
  const scenarioCommands = useMemo<DynamicCommand[]>(() => {
    if (page !== "project" || !activeProjectId) return [];
    return scenarios.map((s) => ({
      id: `sc-${s.id}`,
      label: s.name,
      description: lineageLabel(scenarios, s.id),
      category: "Scenarios",
      action: "switch-scenario",
      scenarioId: s.id,
    }));
  }, [page, activeProjectId, scenarios]);

  // Combined command pool (page-context first, then static commands).
  const ALL_COMMANDS: DynamicCommand[] = useMemo(
    () => [
      ...pageCommands,
      ...scenarioCommands,
      ...(allCommands as DynamicCommand[]),
    ],
    [pageCommands, scenarioCommands, allCommands],
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: the deps are the inputs the visible result list derives from — the cursor must reset whenever they change.
  useEffect(() => {
    setActiveIdx(0);
  }, [query, ALL_COMMANDS, elementMatches]);

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

      // ── Clear results (confirmed by the app-level modal) ───────────────
      if (cmd.id === "a-clear-results" || cmd.id === "a-clear-all-results") {
        if (!activeProjectId) return;
        const all = cmd.id === "a-clear-all-results";
        const scenario = scenarios.find((s) => s.id === activeScenarioId);
        requestClearResults({
          projectId: activeProjectId,
          scope: all ? "all" : "target",
          scenarioId: activeScenarioId,
          name: all
            ? (projects.find((p) => p.id === activeProjectId)?.name ??
              "this project")
            : (scenario?.name ?? "Base model"),
        });
        return;
      }

      // ── Replace this project's network ─────────────────────────────────
      if (cmd.id === "a4") {
        // The picker filter and the parser both follow the open project's
        // engine — `.inp` alone does not say which model format a file is.
        if (!activeProjectEngine) return;
        openAndLoadNetwork(activeProjectEngine)
          .then((imported) => {
            if (imported) {
              bumpNetwork();
              const { network, findings } = imported;
              // A model that read but is not yet simulable must not report as
              // a plain success — the Issues panel is where it gets resolved.
              showToast(
                findings.length > 0
                  ? `Loaded ${network.nodes.length} nodes · ${findings.length} issue${findings.length === 1 ? "" : "s"} to resolve`
                  : `Loaded ${network.nodes.length} nodes`,
                findings.length > 0 ? "warn" : "success",
              );
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

      // ── Open documentation in the browser ──────────────────────────────
      if (cmd.id === "a-docs") {
        void openUrl(DOCS_URL);
        return;
      }

      // ── Export INP / results CSV (native save dialogs in the backend) ───
      if (cmd.id === "a-export-inp" || cmd.id === "a-export-csv") {
        if (!activeProjectId) return;
        const command =
          cmd.id === "a-export-inp"
            ? "export_project_inp"
            : "export_results_csv";
        void tryInvoke<string | null>(command, {
          projectId: activeProjectId,
          scenarioId: activeScenarioId ?? null,
        }).then((path) => {
          // `null` = user cancelled the save dialog (or command unavailable).
          if (path) showToast(`Saved ${path}`, "success");
        });
        return;
      }

      // ── Analysis: export GeoJSON ────────────────────────────────────────
      if (cmd.id === "a2") {
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
              properties: {
                id: n.id,
                type: n.type,
                // Static attributes, then result values when available.
                ...(n.elevation != null ? { elevation: n.elevation } : {}),
                ...(n.pressure != null ? { pressure: n.pressure } : {}),
                ...(n.head != null ? { head: n.head } : {}),
                ...(n.demand != null ? { demand: n.demand } : {}),
                ...(n.quality != null ? { quality: n.quality } : {}),
              },
            })),
            ...allLinks.map((l) => {
              const from = nodeCoords.get(l.fromId) ?? [0, 0];
              const to = nodeCoords.get(l.toId) ?? [0, 0];
              return {
                type: "Feature" as const,
                geometry: {
                  type: "LineString" as const,
                  // Intermediate vertices included — a straight from→to
                  // line flattened every polyline conduit.
                  coordinates: [from, ...(l.vertices ?? []), to],
                },
                properties: {
                  id: l.id,
                  type: l.type,
                  // Static attributes, then result values when available.
                  // Absent attributes are omitted, never exported as 0.
                  ...(l.diameter != null && l.diameter > 0
                    ? { diameter: l.diameter }
                    : {}),
                  ...(l.length != null && l.length > 0
                    ? { length: l.length }
                    : {}),
                  // `velocity` is only meaningful once sim data is merged in
                  // — gate it on `flow` like the other result values.
                  ...(l.flow != null
                    ? { flow: l.flow, velocity: l.velocity }
                    : {}),
                  ...(l.status != null ? { status: l.status } : {}),
                  ...(l.quality != null ? { quality: l.quality } : {}),
                },
              };
            }),
            // Areal elements (subcatchments) as closed polygons.
            ...allRegions
              .filter((r) => r.ring.length >= 3)
              .map((r) => ({
                type: "Feature" as const,
                geometry: {
                  type: "Polygon" as const,
                  coordinates: [[...r.ring, r.ring[0]]],
                },
                properties: {
                  id: r.id,
                  type: r.type,
                  ...(r.outletId != null ? { outlet: r.outletId } : {}),
                },
              })),
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
        case "switch-scenario":
          if (cmd.scenarioId) setActiveScenarioId(cmd.scenarioId);
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
          if (activeProjectId) {
            closeProject();
          } else {
            setPage("home");
          }
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
      closeProject,
      activeProjectId,
      activeProjectEngine,
      activeScenarioId,
      requestClearResults,
      projects,
      scenarios,
      setActiveScenarioId,
      setPage,
      setProjectView,
      setTheme,
      openRunModal,
      openScenariosModal,
      resultMeta,
      allNodes,
      allLinks,
      allRegions,
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
    <ModalBackdrop
      onDismiss={closeCommandPalette}
      zIndex={200}
      style={{
        alignItems: "flex-start",
        paddingTop: 80,
        animation: "fadeIn 120ms ease-out",
      }}
    >
      {/* Panel */}
      <div
        {...stopBackdropEvents}
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
              fontSize: "var(--text-xl)",
              fontFamily: "var(--font-ui)",
              outline: "none",
            }}
          />
          <kbd
            style={{
              fontSize: "var(--text-sm)",
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
                fontSize: "var(--text-lg)",
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
                  fontSize: "var(--text-sm)",
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
                          fontSize: "var(--text-lg)",
                          color: "var(--text-primary)",
                        }}
                      >
                        {m.id}
                        <span
                          style={{
                            marginLeft: 8,
                            fontSize: "var(--text-sm)",
                            color: "var(--text-tertiary)",
                            fontFamily: "var(--font-ui)",
                          }}
                        >
                          {m.kind} · {m.subtype}
                        </span>
                      </div>
                      <div
                        style={{
                          fontSize: "var(--text-md)",
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
                      fontSize: "var(--text-sm)",
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
                          fontSize: "var(--text-lg)",
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
                                fontSize: "var(--text-md)",
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
                              fontSize: "var(--text-sm)",
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
            fontSize: "var(--text-sm)",
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
                  fontSize: "var(--text-sm)",
                }}
              >
                {key}
              </kbd>
              {label}
            </span>
          ))}
        </div>
      </div>
    </ModalBackdrop>
  );
}
