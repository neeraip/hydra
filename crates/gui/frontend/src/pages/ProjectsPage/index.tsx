import {
  ChevronDownIcon,
  ChevronUpDownIcon,
  ChevronUpIcon,
  EllipsisHorizontalIcon,
  MagnifyingGlassIcon,
} from "@heroicons/react/16/solid";
import { ChevronLeftIcon, ChevronRightIcon } from "@heroicons/react/20/solid";
import {
  type ColumnFiltersState,
  columnFilteringFeature,
  createColumnHelper,
  createFilteredRowModel,
  createPaginatedRowModel,
  createSortedRowModel,
  filterFn_equalsString,
  filterFn_includesString,
  flexRender,
  globalFilteringFeature,
  type PaginationState,
  type RowSelectionState,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
  type SortingState,
  sortFn_basic,
  tableFeatures,
  useTable,
} from "@tanstack/react-table";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useAppState } from "../../AppContext";
import { DeleteConfirmModal } from "../../components/modals/DeleteConfirmModal";
import { DeleteProjectModal } from "../../components/modals/DeleteProjectModal";
import { ImportArchiveWizard } from "../../components/modals/ImportArchiveWizard";
import { NewProjectWizard } from "../../components/modals/NewProjectWizard";
import { NewProjectButton } from "../../components/ui/NewProjectButton";
import { RowMenu } from "../../components/ui/RowMenu";
import {
  type ArchiveScan,
  deleteAllSimulations,
  deleteProjectOnDisk,
  type ImportedModel,
  openAndScanArchive,
  openBaseFolder,
  type Project,
  type ProjectState,
  projectsResultsSize,
  renameProjectOnDisk,
  useEngines,
  useProjects,
} from "../../hooks";
import { fetchInto } from "../../hooks/fetchInto";
import { formatIpcError } from "../../hooks/ipc";
import { PROJECTS_SEARCH_INPUT_ID } from "../../shortcuts";
import { formatBytes } from "../../units";
import { ContextMenu, type ContextMenuState } from "./ContextMenu";

const STATE_LABELS: Record<ProjectState, string> = {
  draft: "Draft",
  ready: "Ready",
  simulated: "Simulated",
  running: "Running",
  failed: "Failed",
  stale: "Edited",
};

const STATE_COLORS: Record<ProjectState, string> = {
  draft: "var(--text-tertiary)",
  ready: "var(--accent)",
  simulated: "var(--status-success)",
  running: "var(--accent)",
  failed: "var(--status-error)",
  stale: "#f59e0b",
};

/**
 * Whether the select-all checkbox shows its indeterminate dash.
 *
 * Its own state is the answer to "some but not all". TanStack Table v8 read
 * that straight off `getIsSomeRowsSelected()`, which excluded the all-selected
 * case for you. Since v9 that method means "at least one" and stays true once
 * every row is selected, so the exclusion has to be written here or the box
 * never leaves the dash.
 */
export function selectAllIsIndeterminate(
  someSelected: boolean,
  allSelected: boolean,
): boolean {
  return someSelected && !allSelected;
}

// ── Table features ───────────────────────────────────────────────────────────

// v9 registers only the features a table actually uses, and the row models
// and filter/sort functions ride along in the same call. Every name used as
// a string below (`filterFn: "equalsString"`, `sortFn: "basic"`, the table's
// `globalFilterFn`) has to be registered here or it does not resolve.
const features = tableFeatures({
  columnFilteringFeature,
  globalFilteringFeature,
  rowSortingFeature,
  rowPaginationFeature,
  rowSelectionFeature,
  filteredRowModel: createFilteredRowModel(),
  sortedRowModel: createSortedRowModel(),
  paginatedRowModel: createPaginatedRowModel(),
  filterFns: {
    equalsString: filterFn_equalsString,
    includesString: filterFn_includesString,
  },
  sortFns: { basic: sortFn_basic },
});

// ── Column helper ─────────────────────────────────────────────────────────────

const col = createColumnHelper<typeof features, Project>();

const CHECKBOX_STYLE: React.CSSProperties = {
  accentColor: "var(--accent)",
  width: 13,
  height: 13,
  cursor: "pointer",
  verticalAlign: "middle",
};

// ── Main page ────────────────────────────────────────────────────────────────

export function ProjectsPage() {
  const { projectsVersion, openProject, bumpProjects, showToast } =
    useAppState();
  const [showWizard, setShowWizard] = useState(false);
  // A model recognised before the wizard opened, so it can start from what
  // was read rather than asking for it again.
  const [wizardModel, setWizardModel] = useState<ImportedModel | null>(null);
  // A scanned archive awaiting review; the modal owns the rest.
  const [archiveScan, setArchiveScan] = useState<ArchiveScan | null>(null);

  /** Pick a .zip of models and open the review on what the scan found. */
  async function importArchive() {
    try {
      const scan = await openAndScanArchive();
      if (!scan) return; // cancelled
      setArchiveScan(scan);
    } catch (e) {
      showToast(formatIpcError(e), "error");
    }
  }

  // ── Bulk actions on the checkbox selection ───────────────────────────────
  const runBulk = useCallback(
    async (kind: "clear" | "delete", targets: Project[]) => {
      setPendingBulk(null);
      // Sequential rather than concurrent: each project's work touches the
      // filesystem, and a failure part-way should leave a comprehensible
      // state rather than an arbitrary interleaving.
      let done = 0;
      const failures: string[] = [];
      for (const project of targets) {
        try {
          if (kind === "clear") {
            await deleteAllSimulations(project.id);
          } else if (!(await deleteProjectOnDisk(project.id))) {
            // The list is a filesystem scan, so a row that cannot be deleted
            // is a genuine failure rather than an already-absent project.
            failures.push(`${project.name}: could not be deleted`);
            continue;
          }
          done += 1;
        } catch (err) {
          // Reported per project, not as one opaque failure: the usual cause
          // is a simulation still running for that target, which names the
          // project the user has to deal with.
          failures.push(`${project.name}: ${formatIpcError(err)}`);
        }
      }
      setRowSelection({});
      bumpProjects();

      const noun = kind === "clear" ? "cleared" : "deleted";
      if (failures.length === 0) {
        showToast(
          `${done} project${done === 1 ? "" : "s"} ${noun}`,
          done === 0 ? "info" : "success",
        );
      } else {
        showToast(
          `${done} ${noun}, ${failures.length} failed: ${failures[0]}`,
          "error",
        );
      }
    },
    [bumpProjects, showToast],
  );

  const handleOpenProject = useCallback(
    (id: string) => {
      // Navigate immediately; AppContext loads and primes network data in the background.
      openProject(id);
    },
    [openProject],
  );
  const projects = useProjects(projectsVersion);

  const [globalFilter, setGlobalFilter] = useState("");
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [sorting, setSorting] = useState<SortingState>([
    { id: "modified", desc: true },
  ]);
  const [stateFilter, setStateFilter] = useState<string>("all");
  const [pagination, setPagination] = useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  });
  // Keyed by project id (see `getRowId`), so a selection survives sorting,
  // filtering and paging instead of following row positions.
  const [rowSelection, setRowSelection] = useState<RowSelectionState>({});
  /** Pending bulk action awaiting confirmation. */
  const [pendingBulk, setPendingBulk] = useState<"clear" | "delete" | null>(
    null,
  );
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [pendingDeleteProject, setPendingDeleteProject] =
    useState<Project | null>(null);

  const handleRowContextMenu = useCallback(
    (e: React.MouseEvent, project: Project) => {
      e.preventDefault();
      setContextMenu({ project, x: e.clientX, y: e.clientY });
    },
    [],
  );

  // Combine state dropdown filter into columnFilters
  const effectiveFilters = useMemo<ColumnFiltersState>(() => {
    const f: ColumnFiltersState = [...columnFilters];
    if (stateFilter !== "all") f.push({ id: "state", value: stateFilter });
    return f;
  }, [columnFilters, stateFilter]);

  const engines = useEngines();
  const columns = useMemo(
    () =>
      // `col.columns` rather than a bare array: it preserves each column's
      // own value type instead of widening them all to one union.
      col.columns([
        col.display({
          id: "select",
          header: ({ table }) => (
            <input
              type="checkbox"
              aria-label="Select all projects"
              checked={table.getIsAllRowsSelected()}
              ref={(el) => {
                // Indeterminate is not an attribute — it only exists on the
                // DOM node, so it has to be set imperatively.
                if (el)
                  el.indeterminate = selectAllIsIndeterminate(
                    table.getIsSomeRowsSelected(),
                    table.getIsAllRowsSelected(),
                  );
              }}
              onChange={table.getToggleAllRowsSelectedHandler()}
              style={CHECKBOX_STYLE}
            />
          ),
          cell: (info) => (
            <input
              type="checkbox"
              aria-label={`Select ${info.row.original.name}`}
              checked={info.row.getIsSelected()}
              onChange={info.row.getToggleSelectedHandler()}
              // The row toggles selection too, so a bubbling checkbox click
              // would toggle twice and land back where it started.
              onClick={(e) => e.stopPropagation()}
              style={CHECKBOX_STYLE}
            />
          ),
        }),
        col.display({
          id: "engine",
          // No header: the glyph is an identity mark, and a column of two
          // letters needs no word above it — the tooltip names the engine.
          header: () => null,
          cell: (info) => {
            const p = info.row.original;
            const engine = engines.find((e) => e.key === p.engine) ?? null;
            return (
              <span
                data-tooltip={engine?.label ?? "Unsupported engine"}
                style={{
                  display: "inline-block",
                  width: "100%",
                  textAlign: "center",
                  fontSize: "var(--text-xs)",
                  fontWeight: 700,
                  letterSpacing: "0.04em",
                  fontFamily: "var(--font-ui)",
                  color: engine?.accent ?? "var(--text-tertiary)",
                }}
              >
                {engine?.pill ?? "??"}
              </span>
            );
          },
        }),
        col.accessor("name", {
          header: "Name",
          cell: (info) => {
            const p = info.row.original;
            return (
              <span
                style={{ display: "inline-flex", alignItems: "center", gap: 6 }}
              >
                <button
                  type="button"
                  onClick={(e) => {
                    // The row toggles selection; opening is a different intent.
                    e.stopPropagation();
                    if (!p.folderMissing) handleOpenProject(p.id);
                  }}
                  style={{
                    background: "none",
                    border: "none",
                    padding: 0,
                    cursor: p.folderMissing ? "default" : "pointer",
                    color: p.folderMissing
                      ? "var(--text-disabled)"
                      : "var(--accent)",
                    fontFamily: "var(--font-ui)",
                    fontSize: "var(--text-lg)",
                    fontWeight: 500,
                    textAlign: "left",
                    opacity: p.folderMissing ? 0.5 : 1,
                  }}
                >
                  {info.getValue()}
                </button>
                {p.folderMissing && (
                  <span
                    style={{
                      fontSize: "var(--text-xs)",
                      fontWeight: 700,
                      letterSpacing: "0.05em",
                      padding: "1px 5px",
                      borderRadius: 3,
                      background: "#f59e0b26",
                      border: "1px solid #f59e0b55",
                      color: "var(--status-warn, #f59e0b)",
                    }}
                  >
                    MISSING
                  </span>
                )}
              </span>
            );
          },
          enableSorting: true,
        }),
        col.accessor("state", {
          header: "State",
          cell: (info) => {
            const s = info.getValue();
            return (
              <span
                style={{ fontSize: "var(--text-md)", color: STATE_COLORS[s] }}
              >
                {s === "simulated" || s === "running" ? "● " : "○ "}
                {STATE_LABELS[s]}
              </span>
            );
          },
          filterFn: "equalsString",
          enableSorting: true,
        }),
        col.accessor("nodeCount", {
          header: "Nodes",
          cell: (info) => (
            <span
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: "var(--text-md)",
              }}
            >
              {info.getValue().toLocaleString()}
            </span>
          ),
          enableSorting: true,
        }),
        col.accessor("linkCount", {
          header: "Links",
          cell: (info) => (
            <span
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: "var(--text-md)",
              }}
            >
              {info.getValue().toLocaleString()}
            </span>
          ),
          enableSorting: true,
        }),
        col.accessor("scenarioCount", {
          header: "Scenarios",
          cell: (info) => (
            <span
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: "var(--text-md)",
              }}
            >
              {info.getValue()}
            </span>
          ),
          enableSorting: true,
        }),
        // Modified / Last run sort on the numeric epoch-ms fields (the labels
        // are human strings — "2 days ago" vs "Just now" sorts alphabetically).
        // `sortUndefined: "last"` keeps never-run / missing values at the
        // bottom in both directions; the cell still shows the friendly label.
        col.accessor((p) => msSortValue(p.modifiedAtMs), {
          id: "modified",
          header: "Modified",
          cell: (info) => (
            <span
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-secondary)",
              }}
            >
              {info.row.original.modifiedLabel}
            </span>
          ),
          sortFn: "basic",
          sortUndefined: "last",
          enableSorting: true,
        }),
        col.accessor((p) => msSortValue(p.lastRunAtMs), {
          id: "lastRun",
          header: "Last run",
          cell: (info) => (
            <span
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
              }}
            >
              {info.row.original.lastRunLabel ?? "—"}
            </span>
          ),
          sortFn: "basic",
          sortUndefined: "last",
          enableSorting: true,
        }),
        // Kebab actions column — hover/focus-revealed "…" button that opens
        // the same context menu as right-click, anchored at the button.
        col.display({
          id: "actions",
          header: "",
          cell: (info) => (
            <button
              type="button"
              className="row-kebab"
              aria-label={`Actions for ${info.row.original.name}`}
              aria-haspopup="menu"
              onClick={(e) => {
                e.stopPropagation();
                const rect = e.currentTarget.getBoundingClientRect();
                setContextMenu({
                  project: info.row.original,
                  x: rect.left,
                  y: rect.bottom + 4,
                });
              }}
            >
              <EllipsisHorizontalIcon style={{ width: 15, height: 15 }} />
            </button>
          ),
        }),
      ]),
    [handleOpenProject, engines],
  );

  const table = useTable({
    features,
    data: projects,
    columns,
    state: {
      globalFilter,
      columnFilters: effectiveFilters,
      sorting,
      pagination,
      rowSelection,
    },
    // Without this, TanStack keys selection by row index — sorting or
    // filtering would then silently move the selection to different projects.
    getRowId: (p) => p.id,
    onRowSelectionChange: setRowSelection,
    onGlobalFilterChange: setGlobalFilter,
    onColumnFiltersChange: setColumnFilters,
    onSortingChange: setSorting,
    onPaginationChange: setPagination,
    globalFilterFn: "includesString",
  });

  const selectedProjects = table
    .getSelectedRowModel()
    .rows.map((r) => r.original);
  // `getSelectedRowModel` rebuilds its array every render, so the sizing
  // effect below is keyed on the ids' *contents* — otherwise it would refire
  // on every keystroke in the search box. Derived from the row model rather
  // than the selection state so ids whose project no longer exists are
  // already dropped.
  const selectedKey = selectedProjects.map((p) => p.id).join(",");
  const selectedIds = useMemo(
    () => (selectedKey === "" ? [] : selectedKey.split(",")),
    [selectedKey],
  );

  // Sized whenever a selection exists, because the figure now labels the menu
  // entry rather than the confirmation. One call for the whole selection: the
  // stat work is negligible, round trips are not, and a selection is
  // unbounded.
  const [bulkBytes, setBulkBytes] = useState<number | null>(null);
  useEffect(() => {
    if (selectedIds.length === 0) {
      setBulkBytes(null);
      return;
    }
    setBulkBytes(null);
    return fetchInto(projectsResultsSize(selectedIds), setBulkBytes);
  }, [selectedIds]);

  const { rows } = table.getRowModel();
  const pageCount = table.getPageCount();
  const pageIndex = table.state.pagination.pageIndex;
  const canPrev = table.getCanPreviousPage();
  const canNext = table.getCanNextPage();

  const stateOptions: Array<"all" | ProjectState> = [
    "all",
    "draft",
    "ready",
    "simulated",
    "running",
    "failed",
    "stale",
  ];

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        background: "var(--bg-app)",
      }}
    >
      {/* ── Toolbar ─────────────────────────────────────────────────────── */}
      <div
        style={{
          height: 52,
          flexShrink: 0,
          padding: "0 10px",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-panel)",
          display: "flex",
          alignItems: "center",
          gap: 10,
        }}
      >
        {/* New Project button — the main action opens the wizard; the
            caret starts from a model file instead, which names its own
            engine. */}
        <NewProjectButton
          size="sm"
          onNew={() => {
            setWizardModel(null);
            setShowWizard(true);
          }}
          onImported={(model) => {
            setWizardModel(model);
            setShowWizard(true);
          }}
          onArchive={() => void importArchive()}
          onError={(message) => showToast(message, "error")}
        />

        {/* Global search */}
        <div style={{ position: "relative", flex: "0 1 280px" }}>
          <MagnifyingGlassIcon
            style={{
              width: 13,
              height: 13,
              position: "absolute",
              left: 8,
              top: "50%",
              transform: "translateY(-50%)",
              color: "var(--text-tertiary)",
              pointerEvents: "none",
            }}
          />
          <input
            id={PROJECTS_SEARCH_INPUT_ID}
            value={globalFilter}
            onChange={(e) => setGlobalFilter(e.target.value)}
            placeholder="Search projects…"
            style={{
              width: "100%",
              height: 28,
              paddingLeft: 26,
              paddingRight: 8,
              border: "1px solid var(--border)",
              borderRadius: 5,
              background: "var(--bg-input)",
              color: "var(--text-primary)",
              fontSize: "var(--text-md)",
              fontFamily: "var(--font-ui)",
              outline: "none",
              boxSizing: "border-box",
            }}
            onFocus={(e) =>
              (e.currentTarget.style.borderColor = "var(--border-focus)")
            }
            onBlur={(e) =>
              (e.currentTarget.style.borderColor = "var(--border)")
            }
          />
        </div>

        {/* State filter */}
        <select
          value={stateFilter}
          onChange={(e) => setStateFilter(e.target.value)}
          style={selectStyle}
        >
          <option value="all">All states</option>
          {stateOptions
            .filter((v) => v !== "all")
            .map((v) => (
              <option key={v} value={v}>
                {STATE_LABELS[v as ProjectState]}
              </option>
            ))}
        </select>

        <div style={{ flex: 1 }} />

        {/* Selection actions — replace the row count while a selection is
            live, so the toolbar does not grow a permanently empty slot. */}
        {selectedProjects.length > 0 ? (
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 8,
              flexShrink: 0,
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
            }}
          >
            {selectedProjects.length} selected
            <button
              type="button"
              className="btn-link"
              style={{ fontSize: "var(--text-sm)" }}
              onClick={() => setRowSelection({})}
            >
              Clear
            </button>
            <RowMenu
              label="Actions for selected projects"
              items={[
                {
                  label: "Clear simulation results",
                  detail:
                    bulkBytes === null
                      ? undefined
                      : `Frees ${formatBytes(bulkBytes)}`,
                  onSelect: () => setPendingBulk("clear"),
                  disabled: !selectedProjects.some(
                    (p) => p.state === "simulated",
                  ),
                  disabledReason: "None of these have been simulated",
                  danger: true,
                },
                {
                  label: "Delete projects",
                  onSelect: () => setPendingBulk("delete"),
                  danger: true,
                },
              ]}
            />
          </span>
        ) : (
          /* Row count */
          <span
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-tertiary)",
              flexShrink: 0,
            }}
          >
            {table.getFilteredRowModel().rows.length} project
            {table.getFilteredRowModel().rows.length !== 1 ? "s" : ""}
          </span>
        )}

        {/* Page size */}
        <select
          value={pagination.pageSize}
          onChange={(e) => table.setPageSize(Number(e.target.value))}
          style={selectStyle}
        >
          {[10, 20, 50, 100].map((n) => (
            <option key={n} value={n}>
              {n} / page
            </option>
          ))}
        </select>
      </div>

      {/* ── Table ───────────────────────────────────────────────────────── */}
      <div style={{ flex: 1, overflow: "auto" }}>
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            fontSize: "var(--text-lg)",
          }}
        >
          <thead>
            {table.getHeaderGroups().map((hg) => (
              <tr
                key={hg.id}
                style={{ borderBottom: "1px solid var(--border)" }}
              >
                {hg.headers.map((header) => {
                  const sorted = header.column.getIsSorted();
                  const canSort = header.column.getCanSort();
                  const label = (
                    <>
                      {flexRender(
                        header.column.columnDef.header,
                        header.getContext(),
                      )}
                      {canSort &&
                        (sorted === "asc" ? (
                          <ChevronUpIcon style={{ width: 12, height: 12 }} />
                        ) : sorted === "desc" ? (
                          <ChevronDownIcon style={{ width: 12, height: 12 }} />
                        ) : (
                          <ChevronUpDownIcon
                            style={{ width: 12, height: 12, opacity: 0.4 }}
                          />
                        ))}
                    </>
                  );
                  return (
                    <th
                      key={header.id}
                      aria-sort={
                        canSort
                          ? sorted === "asc"
                            ? "ascending"
                            : sorted === "desc"
                              ? "descending"
                              : "none"
                          : undefined
                      }
                      style={{
                        padding:
                          header.column.id === "engine"
                            ? "8px 4px"
                            : "8px 14px",
                        width: header.column.id === "engine" ? 34 : undefined,
                        textAlign: "left",
                        fontWeight: 600,
                        fontSize: "var(--text-sm)",
                        letterSpacing: "0.05em",
                        textTransform: "uppercase",
                        color: "var(--text-tertiary)",
                        background: "var(--bg-panel)",
                        position: "sticky",
                        top: 0,
                        zIndex: 1,
                        borderBottom: "1px solid var(--border)",
                        userSelect: "none",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {canSort ? (
                        // Real <button> so Enter/Space toggle sorting natively;
                        // .th-sort-btn inherits every font style, so layout is
                        // identical to the previous bare label.
                        <button
                          type="button"
                          className="th-sort-btn"
                          style={{ width: "auto" }}
                          onClick={header.column.getToggleSortingHandler()}
                        >
                          {label}
                        </button>
                      ) : (
                        <span
                          style={{
                            display: "inline-flex",
                            alignItems: "center",
                            gap: 4,
                          }}
                        >
                          {label}
                        </span>
                      )}
                    </th>
                  );
                })}
              </tr>
            ))}
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td
                  colSpan={columns.length}
                  style={{
                    padding: "40px 14px",
                    textAlign: "center",
                    color: "var(--text-tertiary)",
                    fontSize: "var(--text-lg)",
                  }}
                >
                  {projects.length === 0
                    ? "No projects yet."
                    : "No projects match the current filters."}
                </td>
              </tr>
            ) : (
              rows.map((row, i) => (
                <tr
                  key={row.id}
                  className="projects-row"
                  style={{
                    background:
                      i % 2 === 0 ? "var(--bg-app)" : "var(--bg-panel)",
                    borderBottom: "1px solid var(--border)",
                    transition: "background var(--t-fast)",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLTableRowElement).style.background =
                      "var(--bg-card)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLTableRowElement).style.background =
                      i % 2 === 0 ? "var(--bg-app)" : "var(--bg-panel)";
                  }}
                  onContextMenu={(e) => handleRowContextMenu(e, row.original)}
                  onClick={() => row.toggleSelected()}
                >
                  {row.getAllCells().map((cell) => (
                    <td
                      key={cell.id}
                      style={{
                        padding:
                          cell.column.id === "engine" ? "8px 4px" : "8px 14px",
                        width: cell.column.id === "engine" ? 34 : undefined,
                        verticalAlign: "middle",
                      }}
                    >
                      {flexRender(
                        cell.column.columnDef.cell,
                        cell.getContext(),
                      )}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* ── Pagination bar ──────────────────────────────────────────────── */}
      {pageCount > 1 && (
        <div
          style={{
            height: 44,
            flexShrink: 0,
            padding: "0 20px",
            borderTop: "1px solid var(--border)",
            background: "var(--bg-panel)",
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          <button
            type="button"
            onClick={() => table.previousPage()}
            disabled={!canPrev}
            className="btn-pager"
          >
            <ChevronLeftIcon style={{ width: 14, height: 14 }} />
          </button>

          {buildPageNumbers(pageIndex, pageCount).map((item, i, items) =>
            item === "…" ? (
              <span
                key={`ellipsis-${items[i - 1] ?? "start"}-${items[i + 1] ?? "end"}`}
                style={{
                  fontSize: "var(--text-md)",
                  color: "var(--text-disabled)",
                  padding: "0 2px",
                }}
              >
                …
              </span>
            ) : (
              <button
                type="button"
                key={item}
                onClick={() => table.setPageIndex(item as number)}
                className={`btn-pager${item === pageIndex ? " pager-active" : ""}`}
                style={{ minWidth: 28 }}
              >
                {(item as number) + 1}
              </button>
            ),
          )}

          <button
            type="button"
            onClick={() => table.nextPage()}
            disabled={!canNext}
            className="btn-pager"
          >
            <ChevronRightIcon style={{ width: 14, height: 14 }} />
          </button>

          <span
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-tertiary)",
              marginLeft: 4,
            }}
          >
            Page {pageIndex + 1} of {pageCount}
          </span>
        </div>
      )}

      {contextMenu && (
        <ContextMenu
          menu={contextMenu}
          onClose={() => setContextMenu(null)}
          onOpen={handleOpenProject}
          onOpenFolder={(id) => openBaseFolder(id)}
          onRemove={(id) => {
            deleteProjectOnDisk(id).then(() => bumpProjects());
            setContextMenu(null);
          }}
          onRename={(id, name) => {
            renameProjectOnDisk(id, name).then(() => bumpProjects());
          }}
          onDelete={(project) => setPendingDeleteProject(project)}
        />
      )}

      <DeleteProjectModal
        open={!!pendingDeleteProject}
        projectName={pendingDeleteProject?.name ?? ""}
        onCancel={() => setPendingDeleteProject(null)}
        onConfirm={() => {
          if (!pendingDeleteProject) return;
          const id = pendingDeleteProject.id;
          setPendingDeleteProject(null);
          deleteProjectOnDisk(id).then(() => bumpProjects());
        }}
      />

      <DeleteConfirmModal
        open={pendingBulk !== null}
        elementKind="projects"
        elementId={`${selectedProjects.length} projects`}
        title={
          pendingBulk === "clear"
            ? "Clear Simulation Results"
            : "Delete Projects"
        }
        message={
          pendingBulk === "clear" ? (
            <>
              Delete the simulation results for the base model and every
              scenario in{" "}
              <strong style={{ color: "var(--text-primary)" }}>
                {selectedProjects.length} project
                {selectedProjects.length === 1 ? "" : "s"}
              </strong>
              ? The networks themselves are not changed, so the runs can be
              repeated.
            </>
          ) : (
            <>
              Permanently delete{" "}
              <strong style={{ color: "var(--text-primary)" }}>
                {selectedProjects.length} project
                {selectedProjects.length === 1 ? "" : "s"}
              </strong>
              , including every scenario, network and result they contain? This
              cannot be undone.
            </>
          )
        }
        confirmLabel={pendingBulk === "clear" ? "Clear results" : "Delete"}
        onConfirm={() => {
          if (pendingBulk) void runBulk(pendingBulk, selectedProjects);
        }}
        onCancel={() => setPendingBulk(null)}
      />

      {showWizard && (
        <NewProjectWizard
          initial={wizardModel}
          onClose={() => {
            setShowWizard(false);
            bumpProjects();
          }}
        />
      )}

      {archiveScan && (
        <ImportArchiveWizard
          scan={archiveScan}
          onClose={() => setArchiveScan(null)}
          onDone={(created) => {
            setArchiveScan(null);
            bumpProjects();
            showToast(
              created === 1
                ? "Created 1 project from the archive"
                : `Created ${created} projects from the archive`,
              "success",
            );
          }}
        />
      )}
    </div>
  );
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/**
 * Numeric sort value for an optional epoch-ms field: finite numbers pass
 * through, anything else becomes `undefined` so tanstack's
 * `sortUndefined: "last"` keeps missing values at the bottom in both
 * directions.
 */
export function msSortValue(v: number | null | undefined): number | undefined {
  return typeof v === "number" && Number.isFinite(v) ? v : undefined;
}

export function buildPageNumbers(
  current: number,
  total: number,
): Array<number | "…"> {
  if (total <= 7) return Array.from({ length: total }, (_, i) => i);
  const pages: Array<number | "…"> = [];
  const add = (n: number) => {
    if (!pages.includes(n)) pages.push(n);
  };
  add(0);
  if (current > 2) pages.push("…");
  for (
    let i = Math.max(1, current - 1);
    i <= Math.min(total - 2, current + 1);
    i++
  )
    add(i);
  if (current < total - 3) pages.push("…");
  add(total - 1);
  return pages;
}

const selectStyle: React.CSSProperties = {
  height: 28,
  padding: "0 8px",
  border: "1px solid var(--border)",
  borderRadius: 5,
  background: "var(--bg-input)",
  color: "var(--text-secondary)",
  fontSize: "var(--text-md)",
  fontFamily: "var(--font-ui)",
  cursor: "pointer",
  outline: "none",
};

// (pager buttons now use the CSS `.btn-pager` class; no helper needed)
