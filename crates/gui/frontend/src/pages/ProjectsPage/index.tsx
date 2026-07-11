import {
  ChevronDownIcon,
  ChevronUpDownIcon,
  ChevronUpIcon,
  MagnifyingGlassIcon,
} from "@heroicons/react/16/solid";
import { ChevronLeftIcon, ChevronRightIcon } from "@heroicons/react/20/solid";
import {
  type ColumnFiltersState,
  createColumnHelper,
  flexRender,
  getCoreRowModel,
  getFilteredRowModel,
  getPaginationRowModel,
  getSortedRowModel,
  type SortingState,
  useReactTable,
} from "@tanstack/react-table";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useAppState } from "../../AppContext";
import { NewProjectWizard } from "../../components/modals/NewProjectWizard";
import { SplitActionButton } from "../../components/ui/SplitActionButton";
import {
  createProjectOnDisk,
  deleteProjectOnDisk,
  formatInpImportError,
  openAndLoadNetwork,
  openBaseFolder,
  type Project,
  type ProjectState,
  reconcileProjects,
  renameProjectOnDisk,
  useNetworkVersion,
  useProjects,
} from "../../hooks";
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

// ── Column helper ─────────────────────────────────────────────────────────────

const col = createColumnHelper<Project>();

// ── Main page ────────────────────────────────────────────────────────────────

export function ProjectsPage() {
  const {
    projectsVersion,
    openProject,
    bumpProjects,
    createProject,
    showToast,
  } = useAppState();
  const [showWizard, setShowWizard] = useState(false);
  const { bumpNetwork } = useNetworkVersion();

  async function handleImportInp() {
    try {
      const result = await openAndLoadNetwork();
      if (!result) return;
      bumpNetwork();
      const id = crypto.randomUUID();
      const name = result.fileStem || "Imported Project";
      const persisted = await createProjectOnDisk({ id, name });
      const project: Project = persisted ?? {
        id,
        name,
        state: "ready",
        scenarioCount: 0,
        modifiedLabel: "Just now",
        nodeCount: result.nodes.length,
        linkCount: result.links.length,
        sourceCrs: "EPSG:4326",
        insights: null,
        folderMissing: false,
      };
      createProject(project);
      bumpProjects();
    } catch (err) {
      showToast(formatInpImportError(err), "error");
    }
  }
  const handleOpenProject = useCallback(
    (id: string) => {
      // Navigate immediately; AppContext loads and primes network data in the background.
      openProject(id);
    },
    [openProject],
  );
  const projects = useProjects(projectsVersion);

  // Reconcile DB with filesystem on first mount.
  useEffect(() => {
    reconcileProjects().then(() => bumpProjects());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bumpProjects]);

  const [globalFilter, setGlobalFilter] = useState("");
  const [columnFilters, setColumnFilters] = useState<ColumnFiltersState>([]);
  const [sorting, setSorting] = useState<SortingState>([
    { id: "modifiedLabel", desc: true },
  ]);
  const [stateFilter, setStateFilter] = useState<string>("all");
  const [pageSize, setPageSize] = useState(20);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);

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

  const columns = useMemo(
    () => [
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
                onClick={() => !p.folderMissing && handleOpenProject(p.id)}
                style={{
                  background: "none",
                  border: "none",
                  padding: 0,
                  cursor: p.folderMissing ? "default" : "pointer",
                  color: p.folderMissing
                    ? "var(--text-disabled)"
                    : "var(--accent)",
                  fontFamily: "var(--font-ui)",
                  fontSize: 13,
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
                    fontSize: 10,
                    fontWeight: 700,
                    letterSpacing: "0.05em",
                    padding: "1px 5px",
                    borderRadius: 3,
                    background: "var(--status-warn, #f59e0b)26",
                    border: "1px solid var(--status-warn, #f59e0b)55",
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
            <span style={{ fontSize: 12, color: STATE_COLORS[s] }}>
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
          <span style={{ fontFamily: "var(--font-mono)", fontSize: 12 }}>
            {info.getValue().toLocaleString()}
          </span>
        ),
        enableSorting: true,
      }),
      col.accessor("linkCount", {
        header: "Links",
        cell: (info) => (
          <span style={{ fontFamily: "var(--font-mono)", fontSize: 12 }}>
            {info.getValue().toLocaleString()}
          </span>
        ),
        enableSorting: true,
      }),
      col.accessor("scenarioCount", {
        header: "Scenarios",
        cell: (info) => (
          <span style={{ fontFamily: "var(--font-mono)", fontSize: 12 }}>
            {info.getValue()}
          </span>
        ),
        enableSorting: true,
      }),
      col.accessor("modifiedLabel", {
        header: "Modified",
        cell: (info) => (
          <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>
            {info.getValue()}
          </span>
        ),
        enableSorting: true,
      }),
      col.accessor("lastRunLabel", {
        header: "Last run",
        cell: (info) => (
          <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            {info.getValue() ?? "—"}
          </span>
        ),
        enableSorting: true,
      }),
    ],
    [handleOpenProject],
  );

  const table = useReactTable({
    data: projects,
    columns,
    state: {
      globalFilter,
      columnFilters: effectiveFilters,
      sorting,
      pagination: { pageIndex: 0, pageSize },
    },
    onGlobalFilterChange: setGlobalFilter,
    onColumnFiltersChange: setColumnFilters,
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getFilteredRowModel: getFilteredRowModel(),
    getSortedRowModel: getSortedRowModel(),
    getPaginationRowModel: getPaginationRowModel(),
    globalFilterFn: "includesString",
  });

  const { rows } = table.getRowModel();
  const pageCount = table.getPageCount();
  const pageIndex = table.getState().pagination.pageIndex;
  const canPrev = table.getCanPreviousPage();
  const canNext = table.getCanNextPage();

  const stateOptions: Array<"all" | ProjectState> = [
    "all",
    "draft",
    "ready",
    "simulated",
    "running",
    "failed",
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
          padding: "0 20px",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-panel)",
          display: "flex",
          alignItems: "center",
          gap: 10,
        }}
      >
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
              fontSize: 12,
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

        {/* New Project button */}
        <SplitActionButton
          size="sm"
          label="+ New project"
          onClick={() => setShowWizard(true)}
          menuItems={[{ label: "Import INP file…", onClick: handleImportInp }]}
        />

        {/* Row count */}
        <span
          style={{ fontSize: 12, color: "var(--text-tertiary)", flexShrink: 0 }}
        >
          {table.getFilteredRowModel().rows.length} project
          {table.getFilteredRowModel().rows.length !== 1 ? "s" : ""}
        </span>

        {/* Page size */}
        <select
          value={pageSize}
          onChange={(e) => setPageSize(Number(e.target.value))}
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
            fontSize: 13,
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
                  return (
                    <th
                      key={header.id}
                      onClick={header.column.getToggleSortingHandler()}
                      style={{
                        padding: "8px 14px",
                        textAlign: "left",
                        fontWeight: 600,
                        fontSize: 11,
                        letterSpacing: "0.05em",
                        textTransform: "uppercase",
                        color: "var(--text-tertiary)",
                        background: "var(--bg-panel)",
                        position: "sticky",
                        top: 0,
                        zIndex: 1,
                        borderBottom: "1px solid var(--border)",
                        cursor: header.column.getCanSort()
                          ? "pointer"
                          : "default",
                        userSelect: "none",
                        whiteSpace: "nowrap",
                      }}
                    >
                      <span
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          gap: 4,
                        }}
                      >
                        {flexRender(
                          header.column.columnDef.header,
                          header.getContext(),
                        )}
                        {header.column.getCanSort() &&
                          (sorted === "asc" ? (
                            <ChevronUpIcon style={{ width: 12, height: 12 }} />
                          ) : sorted === "desc" ? (
                            <ChevronDownIcon
                              style={{ width: 12, height: 12 }}
                            />
                          ) : (
                            <ChevronUpDownIcon
                              style={{ width: 12, height: 12, opacity: 0.4 }}
                            />
                          ))}
                      </span>
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
                    fontSize: 13,
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
                >
                  {row.getVisibleCells().map((cell) => (
                    <td
                      key={cell.id}
                      style={{ padding: "8px 14px", verticalAlign: "middle" }}
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
                  fontSize: 12,
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
              fontSize: 12,
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
        />
      )}

      {showWizard && (
        <NewProjectWizard
          onClose={() => {
            setShowWizard(false);
            bumpProjects();
          }}
        />
      )}
    </div>
  );
}

// ── Helpers ────────────────────────────────────────────────────────────────────

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
  fontSize: 12,
  fontFamily: "var(--font-ui)",
  cursor: "pointer",
  outline: "none",
};

// (pager buttons now use the CSS `.btn-pager` class; no helper needed)
