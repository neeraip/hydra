/**
 * CSV parsing for the spreadsheet preview.
 *
 * The report's CSV renderer emits RFC 4180 — fields quoted when they contain
 * a comma, quote or newline, with `""` for a literal quote — so splitting on
 * commas would tear apart any field holding one. Sections are separated by a
 * blank line and introduced by a `#`-prefixed title row.
 */

/** Parse RFC 4180 CSV into rows of fields. */
export function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;

  for (let i = 0; i < text.length; i++) {
    const ch = text[i];

    if (quoted) {
      if (ch === '"') {
        // A doubled quote inside a quoted field is one literal quote.
        if (text[i + 1] === '"') {
          field += '"';
          i++;
        } else {
          quoted = false;
        }
      } else {
        field += ch;
      }
      continue;
    }

    if (ch === '"') {
      quoted = true;
    } else if (ch === ",") {
      row.push(field);
      field = "";
    } else if (ch === "\n") {
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else if (ch !== "\r") {
      // A lone \r is a CRLF's other half; carriage returns are never data
      // here because the renderer writes \n endings.
      field += ch;
    }
  }

  // Whatever is still buffered is a final row — unless the text ended with a
  // newline, in which case there is nothing pending and no empty row to add.
  if (field !== "" || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

/** Whether a row is the blank line the renderer puts between sections. */
export function isBlankRow(row: readonly string[]): boolean {
  return row.every((cell) => cell.trim() === "");
}

/** Whether a row is a `#`-prefixed section title. */
export function isTitleRow(row: readonly string[]): boolean {
  return row[0]?.startsWith("#") ?? false;
}

/** A title row's text, without the `#` marker the format uses to flag it. */
export function titleText(row: readonly string[]): string {
  return (row[0] ?? "").replace(/^#\s*/, "");
}

/** Widest row in the sheet — the number of columns the grid needs. */
export function columnCount(rows: readonly (readonly string[])[]): number {
  return rows.reduce((max, row) => Math.max(max, row.length), 0);
}

/**
 * Spreadsheet column name for a zero-based index: A, B, … Z, AA, AB, …
 *
 * Bijective base-26, which is not quite base-26: there is no zero digit, so
 * each step borrows a full unit from the next place (Z is followed by AA, not
 * by BA).
 */
export function columnName(index: number): string {
  let name = "";
  let n = index;
  while (n >= 0) {
    name = String.fromCharCode(65 + (n % 26)) + name;
    n = Math.floor(n / 26) - 1;
  }
  return name;
}

/** Whether a cell should be presented as a number — right-aligned, as a
 * spreadsheet would. Blank cells are not numbers. */
export function isNumeric(cell: string): boolean {
  const trimmed = cell.trim();
  if (trimmed === "") return false;
  return Number.isFinite(Number(trimmed));
}
