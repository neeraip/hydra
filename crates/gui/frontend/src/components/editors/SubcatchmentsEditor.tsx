/* Subcatchments editor — sortable table with summary stats and a
   small donut for impervious distribution. */

import { ChevronDownIcon, ChevronUpIcon } from "@heroicons/react/16/solid";
import { useMemo, useState } from "react";
import { useAppState } from "../../AppContext";
import type { Subcatchment } from "../../hooks";

type SortField = keyof Subcatchment;

export function SubcatchmentsEditor({ accent }: { accent: string }) {
  const { showToast } = useAppState();
  const [subcatchments] = useState<Subcatchment[]>([]);
  const [sortField, setSortField] = useState<SortField>("id");
  const [sortAsc, setSortAsc] = useState(true);
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const rows = useMemo(() => {
    const filtered = search
      ? subcatchments.filter((s) =>
          Object.values(s).some((v) =>
            String(v).toLowerCase().includes(search.toLowerCase()),
          ),
        )
      : subcatchments;
    return [...filtered].sort((a, b) => {
      const av = a[sortField],
        bv = b[sortField];
      if (typeof av === "number" && typeof bv === "number")
        return sortAsc ? av - bv : bv - av;
      return sortAsc
        ? String(av).localeCompare(String(bv))
        : String(bv).localeCompare(String(av));
    });
  }, [subcatchments, sortField, sortAsc, search]);

  const totalArea = subcatchments.reduce((a, s) => a + s.area, 0);
  const meanImperv =
    totalArea > 0
      ? subcatchments.reduce((a, s) => a + s.imperv * s.area, 0) / totalArea
      : 0;
  const totalRunoff = subcatchments.reduce((a, s) => a + s.peakRunoff, 0);

  function toggleSort(f: SortField) {
    if (f === sortField) setSortAsc(!sortAsc);
    else {
      setSortField(f);
      setSortAsc(true);
    }
  }

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
      }}
    >
      {/* Top bar */}
      <div
        style={{
          height: 44,
          padding: "0 16px",
          borderBottom: "1px solid var(--border)",
          display: "flex",
          alignItems: "center",
          gap: 16,
          flexShrink: 0,
        }}
      >
        <SummaryStat
          label="Total area"
          value={`${totalArea.toFixed(2)} ha`}
          accent={accent}
        />
        <SummaryStat
          label="Area-weighted impervious"
          value={`${meanImperv.toFixed(0)} %`}
          accent={accent}
        />
        <SummaryStat
          label="Σ peak runoff"
          value={`${totalRunoff.toFixed(0)} L/s`}
          accent={accent}
        />
        <div style={{ flex: 1 }} />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search subcatchments…"
          style={{
            width: 220,
            height: 28,
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            borderRadius: 5,
            padding: "0 8px",
            color: "var(--text-primary)",
            fontFamily: "var(--font-ui)",
            fontSize: 12,
            outline: "none",
          }}
        />
        <button
          type="button"
          onClick={() => showToast("Feature coming soon")}
          style={{
            background: `${accent}26`,
            color: accent,
            border: `1px solid ${accent}55`,
            borderRadius: 5,
            padding: "0 10px",
            height: 28,
            fontSize: 12,
            fontFamily: "var(--font-ui)",
            cursor: "pointer",
          }}
        >
          + Add subcatchment
        </button>
      </div>

      <div style={{ flex: 1, overflow: "auto" }}>
        <table
          style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}
        >
          <thead>
            <tr>
              <Sth
                field="id"
                label="ID"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
              />
              <Sth
                field="area"
                label="Area (ha)"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
                align="right"
              />
              <Sth
                field="imperv"
                label="Imperv (%)"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
                align="right"
              />
              <Sth
                field="width"
                label="Width (m)"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
                align="right"
              />
              <Sth
                field="slope"
                label="Slope (%)"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
                align="right"
              />
              <Sth
                field="manningImp"
                label="n imp"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
                align="right"
              />
              <Sth
                field="manningPerv"
                label="n perv"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
                align="right"
              />
              <Sth
                field="outletNode"
                label="Outlet"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
              />
              <Sth
                field="rainGage"
                label="Gage"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
              />
              <Sth
                field="peakRunoff"
                label="Peak (L/s)"
                f={sortField}
                a={sortAsc}
                on={toggleSort}
                align="right"
              />
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const sel = selectedId === r.id;
              return (
                <tr
                  key={r.id}
                  onClick={() => setSelectedId(sel ? null : r.id)}
                  style={{
                    cursor: "pointer",
                    background: sel ? `${accent}1f` : undefined,
                    borderLeft: sel
                      ? `2px solid ${accent}`
                      : "2px solid transparent",
                  }}
                >
                  <td
                    style={{
                      ...tdStyle,
                      fontWeight: 500,
                      color: "var(--text-primary)",
                    }}
                  >
                    {r.id}
                  </td>
                  <td style={tdR}>{r.area.toFixed(2)}</td>
                  <td style={tdR}>
                    <ImpBar pct={r.imperv} accent={accent} />
                  </td>
                  <td style={tdR}>{r.width}</td>
                  <td style={tdR}>{r.slope.toFixed(2)}</td>
                  <td style={tdR}>{r.manningImp.toFixed(3)}</td>
                  <td style={tdR}>{r.manningPerv.toFixed(3)}</td>
                  <td style={{ ...tdStyle, color: "var(--text-secondary)" }}>
                    {r.outletNode}
                  </td>
                  <td style={{ ...tdStyle, color: "var(--text-tertiary)" }}>
                    {r.rainGage}
                  </td>
                  <td style={{ ...tdR, color: accent, fontWeight: 500 }}>
                    {r.peakRunoff}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          padding: "8px 16px",
          borderTop: "1px solid var(--border)",
          flexShrink: 0,
          fontSize: 12,
          color: "var(--text-tertiary)",
        }}
      >
        Showing {rows.length} of {subcatchments.length} subcatchments
      </div>
    </div>
  );
}

function SummaryStat({
  label,
  value,
  accent,
}: {
  label: string;
  value: string;
  accent: string;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column" }}>
      <span
        style={{
          fontSize: 9,
          textTransform: "uppercase",
          letterSpacing: 0.4,
          color: "var(--text-tertiary)",
        }}
      >
        {label}
      </span>
      <span
        style={{ fontSize: 13, fontFamily: "var(--font-mono)", color: accent }}
      >
        {value}
      </span>
    </div>
  );
}

function ImpBar({ pct, accent }: { pct: number; accent: string }) {
  return (
    <div style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
      <div
        style={{
          width: 60,
          height: 6,
          background: "var(--bg-rail)",
          borderRadius: 3,
          overflow: "hidden",
        }}
      >
        <div style={{ width: `${pct}%`, height: "100%", background: accent }} />
      </div>
      <span
        style={{ fontFamily: "var(--font-mono)", color: "var(--text-primary)" }}
      >
        {pct}
      </span>
    </div>
  );
}

function Sth({
  field,
  label,
  f,
  a,
  on,
  align,
}: {
  field: SortField;
  label: string;
  f: SortField;
  a: boolean;
  on: (f: SortField) => void;
  align?: "left" | "right";
}) {
  const isActive = f === field;
  return (
    <th
      onClick={() => on(field)}
      style={{
        fontSize: 11,
        fontWeight: 500,
        color: isActive ? "var(--text-secondary)" : "var(--text-tertiary)",
        textAlign: align ?? "left",
        padding: "8px 10px",
        borderBottom: "1px solid var(--border)",
        whiteSpace: "nowrap",
        cursor: "pointer",
        userSelect: "none",
        position: "sticky",
        top: 0,
        background: "var(--bg-panel)",
        zIndex: 1,
      }}
    >
      {label}
      {isActive && (
        <span
          style={{
            marginLeft: 4,
            fontSize: 10,
            display: "inline-flex",
            alignItems: "center",
          }}
        >
          {a ? (
            <ChevronUpIcon style={{ width: 12, height: 12 }} />
          ) : (
            <ChevronDownIcon style={{ width: 12, height: 12 }} />
          )}
        </span>
      )}
    </th>
  );
}

const tdStyle: React.CSSProperties = {
  padding: "7px 10px",
  fontSize: 12,
  fontFamily: "var(--font-mono)",
  borderBottom: "1px solid var(--border)",
};
const tdR: React.CSSProperties = { ...tdStyle, textAlign: "right" as const };
