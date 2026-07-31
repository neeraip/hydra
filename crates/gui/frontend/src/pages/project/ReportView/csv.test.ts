import { describe, expect, it } from "vitest";
import {
  columnCount,
  columnName,
  isBlankRow,
  isNumeric,
  isTitleRow,
  parseCsv,
  titleText,
} from "./csv";

describe("parseCsv", () => {
  it("splits plain rows and fields", () => {
    expect(parseCsv("a,b\nc,d")).toEqual([
      ["a", "b"],
      ["c", "d"],
    ]);
  });

  it("keeps a comma inside a quoted field", () => {
    // The whole reason this is not a split(","): the renderer quotes any
    // field containing one.
    expect(parseCsv('"Smith, J",2')).toEqual([["Smith, J", "2"]]);
  });

  it("reads a doubled quote as one literal quote", () => {
    expect(parseCsv('"say ""hi""",1')).toEqual([['say "hi"', "1"]]);
  });

  it("keeps a newline inside a quoted field", () => {
    expect(parseCsv('"two\nlines",x')).toEqual([["two\nlines", "x"]]);
  });

  it("does not add an empty row for a trailing newline", () => {
    expect(parseCsv("a,b\n")).toEqual([["a", "b"]]);
  });

  it("keeps a blank line as its own row", () => {
    // Blank lines separate sections, so they must survive as rows.
    expect(parseCsv("a\n\nb")).toEqual([["a"], [""], ["b"]]);
  });

  it("preserves empty fields", () => {
    expect(parseCsv("a,,c")).toEqual([["a", "", "c"]]);
  });

  it("accepts CRLF endings without leaving carriage returns in the data", () => {
    expect(parseCsv("a,b\r\nc,d")).toEqual([
      ["a", "b"],
      ["c", "d"],
    ]);
  });

  it("reads ragged rows without padding them", () => {
    expect(parseCsv("a,b,c\nd")).toEqual([["a", "b", "c"], ["d"]]);
  });

  it("returns nothing for empty input", () => {
    expect(parseCsv("")).toEqual([]);
  });

  it("parses a report's opening block", () => {
    const csv =
      '# Simulation Report\nJunctions,12,\n\n# Run Summary\n"a,b",3\n';
    expect(parseCsv(csv)).toEqual([
      ["# Simulation Report"],
      ["Junctions", "12", ""],
      [""],
      ["# Run Summary"],
      ["a,b", "3"],
    ]);
  });
});

describe("row classification", () => {
  it("recognises the blank separator row", () => {
    expect(isBlankRow([""])).toBe(true);
    expect(isBlankRow(["", "  "])).toBe(true);
    expect(isBlankRow(["a"])).toBe(false);
  });

  it("recognises a section title row and strips its marker", () => {
    expect(isTitleRow(["# Run Summary"])).toBe(true);
    expect(titleText(["# Run Summary"])).toBe("Run Summary");
  });

  it("does not treat an ordinary row as a title", () => {
    expect(isTitleRow(["Junctions", "12"])).toBe(false);
    expect(isTitleRow([])).toBe(false);
  });
});

describe("columnCount", () => {
  it("takes the widest row, so no cell is cut off", () => {
    expect(columnCount([["a"], ["a", "b", "c"], ["a", "b"]])).toBe(3);
  });

  it("is zero for an empty sheet", () => {
    expect(columnCount([])).toBe(0);
  });
});

describe("columnName", () => {
  it("names the first columns A onward", () => {
    expect(columnName(0)).toBe("A");
    expect(columnName(25)).toBe("Z");
  });

  it("carries into two letters the way a spreadsheet does", () => {
    // Bijective base-26: Z is followed by AA, not BA.
    expect(columnName(26)).toBe("AA");
    expect(columnName(27)).toBe("AB");
    expect(columnName(51)).toBe("AZ");
    expect(columnName(52)).toBe("BA");
    expect(columnName(701)).toBe("ZZ");
    expect(columnName(702)).toBe("AAA");
  });
});

describe("isNumeric", () => {
  it("accepts the numeric forms the renderer emits", () => {
    for (const n of ["12", "-3.5", "0.00001", "1e-9", "  7  "]) {
      expect(isNumeric(n)).toBe(true);
    }
  });

  it("rejects text, blanks and part-numbers", () => {
    for (const s of ["", "   ", "abc", "12 m", "1,2"]) {
      expect(isNumeric(s)).toBe(false);
    }
  });
});
