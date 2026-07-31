/**
 * Spreadsheet preview for the CSV format.
 *
 * The CSV is rendered as a grid rather than as its literal bytes, for the
 * same reason the HTML preview shows a rendered page rather than its source:
 * a preview should show the artefact as its consumer will. A CSV's consumer
 * is a spreadsheet, and raw CSV — quoted fields, ragged section blocks — is
 * close to unreadable for checking whether the numbers are right.
 *
 * Text stays literal, because a text file's consumer really does read it as
 * text.
 */

import { forwardRef, useImperativeHandle, useMemo, useRef } from "react";
import {
  columnCount,
  columnName,
  isBlankRow,
  isNumeric,
  isTitleRow,
  parseCsv,
  titleText,
} from "./csv";

/** Rows rendered before truncating. A report over this is being scanned, not
 * read, and a grid of every cell costs far more than it shows. The export is
 * never truncated — only this preview. */
const MAX_ROWS = 400;

const HEADER_BG = "#eef1f5";
const GRID = "#d3d9e0";
const INK = "#1a222c";

const cellBase: React.CSSProperties = {
  border: `1px solid ${GRID}`,
  padding: "3px 7px",
  fontSize: "var(--text-md)",
  whiteSpace: "nowrap",
  maxWidth: 320,
  overflow: "hidden",
  textOverflow: "ellipsis",
};

/** The blank margin right of the last column.
 *
 * A content-width table left the rest of the pane plain white, so the sheet
 * appeared to stop mid-air with no edge. This absorbs the slack instead:
 * `width: 100%` on one cell makes auto table layout hand it every spare pixel,
 * leaving the real columns at their natural widths. It carries no column
 * letter, because it is margin rather than data — padding with lettered
 * columns would imply the file has more than it does.
 *
 * `maxWidth` is unset: `cellBase` caps data cells at 320px, which would stop
 * this one growing to fill the pane. */
const fillerCell: React.CSSProperties = {
  ...cellBase,
  width: "100%",
  maxWidth: undefined,
  borderRight: "none",
};

/** Imperative handle: scrolling the sheet is the parent's to trigger but the
 *  grid's to perform, since only it knows which row a section landed on. */
export interface CsvPreviewHandle {
  /** Scroll the Nth section (0-based) into view. */
  scrollToSection: (index: number) => void;
}

export const CsvPreview = forwardRef<CsvPreviewHandle, { content: string }>(
  function CsvPreview({ content }, ref) {
    const { rows, columns, truncated, total } = useMemo(() => {
      const all = parseCsv(content);
      return {
        rows: all.slice(0, MAX_ROWS),
        columns: columnCount(all.slice(0, MAX_ROWS)),
        truncated: all.length > MAX_ROWS,
        total: all.length,
      };
    }, [content]);

    const scrollRef = useRef<HTMLDivElement>(null);
    useImperativeHandle(ref, () => ({
      scrollToSection(index: number) {
        // Skip one: the first `#` row is the document title, not a section.
        const row = scrollRef.current?.querySelector(
          `[data-title-row="${index + 1}"]`,
        );
        row?.scrollIntoView({ behavior: "smooth", block: "start" });
      },
    }));

    // Running ordinal of `#` rows, so a section can be found without
    // re-deriving which rows are titles at scroll time.
    let titleOrdinal = -1;

    return (
      <div
        ref={scrollRef}
        style={{
          flex: 1,
          overflow: "auto",
          background: "#ffffff",
          color: INK,
          fontFamily: "var(--font-ui)",
        }}
      >
        <table
          style={{
            borderCollapse: "collapse",
            // Sits at the top-left so the sticky headers have a corner to pin
            // against rather than floating over centred content.
            tableLayout: "auto",
            // Full width so the trailing filler has space to claim; the real
            // columns keep their content widths because only the filler asks
            // for any of it.
            width: "100%",
          }}
        >
          <thead>
            <tr>
              <th
                style={{
                  ...cellBase,
                  position: "sticky",
                  top: 0,
                  left: 0,
                  zIndex: 2,
                  background: HEADER_BG,
                  minWidth: 34,
                }}
              />
              {Array.from({ length: columns }, (_, i) => (
                <th
                  key={columnName(i)}
                  style={{
                    ...cellBase,
                    position: "sticky",
                    top: 0,
                    zIndex: 1,
                    background: HEADER_BG,
                    fontWeight: 600,
                    color: "#54607a",
                    textAlign: "center",
                  }}
                >
                  {columnName(i)}
                </th>
              ))}
              <th
                style={{
                  ...fillerCell,
                  position: "sticky",
                  top: 0,
                  zIndex: 1,
                  background: HEADER_BG,
                }}
              />
            </tr>
          </thead>
          <tbody>
            {rows.map((row, r) => {
              const blank = isBlankRow(row);
              const title = !blank && isTitleRow(row);
              if (title) titleOrdinal += 1;
              const ordinal = title ? titleOrdinal : undefined;
              return (
                <tr
                  // Row position is the identity here: the sheet is a fixed
                  // grid, not a keyed list, and rows carry no id of their own.
                  // biome-ignore lint/suspicious/noArrayIndexKey: grid position IS the identity
                  key={r}
                  data-title-row={ordinal}
                  style={{ height: blank ? 10 : undefined }}
                >
                  <td
                    style={{
                      ...cellBase,
                      position: "sticky",
                      left: 0,
                      zIndex: 1,
                      background: HEADER_BG,
                      color: "#54607a",
                      textAlign: "right",
                      fontVariantNumeric: "tabular-nums",
                    }}
                  >
                    {r + 1}
                  </td>
                  {Array.from({ length: columns }, (_, c) => {
                    const cell = row[c] ?? "";
                    return (
                      <td
                        // Same reasoning as the row key: a cell is its position.
                        // biome-ignore lint/suspicious/noArrayIndexKey: grid position IS the identity
                        key={c}
                        title={cell.length > 40 ? cell : undefined}
                        style={{
                          ...cellBase,
                          background: title ? "#f6f8fa" : undefined,
                          fontWeight: title && c === 0 ? 600 : undefined,
                          textAlign: isNumeric(cell) ? "right" : "left",
                          fontVariantNumeric: isNumeric(cell)
                            ? "tabular-nums"
                            : undefined,
                        }}
                      >
                        {title && c === 0 ? titleText(row) : cell}
                      </td>
                    );
                  })}
                  <td
                    style={{
                      ...fillerCell,
                      background: title ? "#f6f8fa" : undefined,
                    }}
                  />
                </tr>
              );
            })}
          </tbody>
        </table>

        {truncated ? (
          <p
            style={{
              margin: 0,
              padding: "8px 12px",
              fontSize: "var(--text-sm)",
              color: "#54607a",
            }}
          >
            Showing the first {MAX_ROWS} of {total} rows. The exported file
            contains all of them.
          </p>
        ) : null}
      </div>
    );
  },
);
