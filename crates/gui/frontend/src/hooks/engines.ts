/**
 * Engine registry (hydra-common spec §2): descriptors for every engine
 * compiled into the backend, fetched once per session via `list_engines`.
 * All engine-identity presentation (label, pill, accent) derives from this
 * registry keyed by each project's `engine` key — never hardcoded.
 *
 * An unresolvable key is an explicit unsupported state (e.g. a project
 * created by a newer Hydra carrying an engine this build lacks):
 * `engineByKey` returns null and surfaces must render an unsupported
 * treatment — never fall back to a default engine.
 */

import { useEffect, useState } from "react";
import { registerElementBadges } from "../types/elementTypes";
import { fetchInto } from "./fetchInto";
import { tryInvokeOr } from "./ipc";
import type { OptionKind } from "./reports";
import type { GenericQuantity } from "./results";

/** One source-model file format an engine imports (hydra-common spec §2.2).
 *
 * A file-picker filter, never a validity test — `wds` and `uds` both claim
 * `inp` with incompatible contents, so only the backend parse can decide
 * whether a chosen file is really that engine's model. */
export interface ImportFormat {
  label: string;
  /** Lowercase, no leading dot. */
  extensions: string[];
}

/** Whether this build can actually run the engine (hydra-common spec §2.3).
 * `planned` engines are registered so they can be presented, but no
 * project may be created for one. */
export type EngineStatus = "available" | "planned";

export interface EngineInfo {
  key: string;
  label: string;
  pill: string;
  accent: string;
  summary: string;
  status: EngineStatus;
  import: ImportFormat[];
}

/** Registry served outside a Tauri shell (plain `vite` dev server); mirrors
 * the backend's descriptors. */
export const FALLBACK_ENGINES: EngineInfo[] = [
  {
    key: "wds",
    label: "Water Distribution",
    pill: "WD",
    accent: "#4a90d9",
    summary:
      "Pressurized water distribution network simulation — hydraulics, water quality, and energy on the EPANET data model.",
    status: "available",
    import: [{ label: "EPANET input file", extensions: ["inp"] }],
  },
  {
    key: "uds",
    label: "Urban Drainage",
    pill: "UD",
    accent: "#7a6ff0",
    summary:
      "Stormwater and wastewater collection network simulation — runoff, routing, and water quality on the SWMM data model.",
    status: "available",
    import: [{ label: "SWMM input file", extensions: ["inp"] }],
  },
  {
    key: "och",
    label: "Open Channel",
    pill: "OC",
    accent: "#2f9e9e",
    summary:
      "River and open-channel hydraulics — steady and unsteady flow on the HEC-RAS data model.",
    status: "planned",
    import: [
      {
        label: "HEC-RAS project archive",
        extensions: ["zip", "7z", "tar", "gz", "tgz"],
      },
    ],
  },
];

/** Engines this GUI can open a project for at all: created from an
 * imported model, viewed, and run through the queue. The registry status
 * says only what this build of Hydra can simulate. Mirrors
 * `GUI_OPENABLE_ENGINES` in `commands/projects.rs`.
 *
 * There was a second set here, `GUI_EDITABLE_ENGINES`, listing what this
 * GUI could edit — a third copy of a fact the engine registry's
 * `editing` capabilities already carry, and one nothing in the app read.
 * It went on saying drainage was read-only for as long as it existed,
 * which is what an unread copy does. */
const GUI_OPENABLE_ENGINES: ReadonlySet<string> = new Set(["wds", "uds"]);

/** Whether this build of Hydra can simulate `engine`'s models at all
 * (registry status — not the same as being usable in this GUI). */
export function isEngineAvailable(engine: EngineInfo): boolean {
  return engine.status === "available";
}

/** Whether `engine` can back a new project in this GUI. What can then be
 * done to that project is the engine registry's `editing` capabilities,
 * per operation. */
export function isEngineGuiOpenable(engine: EngineInfo): boolean {
  return isEngineAvailable(engine) && GUI_OPENABLE_ENGINES.has(engine.key);
}

/** The engine's accepted extensions as human-facing text, e.g. ".inp" or
 * ".zip, .7z, .tar". Used to tell the user what a drop zone accepts. */
export function importExtensionLabel(engine: EngineInfo): string {
  const exts = engine.import.flatMap((f) => f.extensions);
  return [...new Set(exts)].map((e) => `.${e}`).join(", ");
}

// The registry is static per build — one fetch per session.
let cached: EngineInfo[] | null = null;

async function getEngines(): Promise<EngineInfo[]> {
  if (cached) return cached;
  const engines = await tryInvokeOr<EngineInfo[]>(
    "list_engines",
    undefined,
    FALLBACK_ENGINES,
  );
  cached = engines.length > 0 ? engines : FALLBACK_ENGINES;
  return cached;
}

/** Descriptor for `key`, or null — an explicit unsupported state. */
export function engineByKey(
  engines: EngineInfo[],
  key: string,
): EngineInfo | null {
  return engines.find((e) => e.key === key) ?? null;
}

/** The engine registry; resolved from the backend once per session. */
export function useEngines(): EngineInfo[] {
  const [engines, setEngines] = useState<EngineInfo[]>(
    () => cached ?? FALLBACK_ENGINES,
  );
  useEffect(() => {
    return fetchInto(getEngines(), setEngines);
  }, []);
  return engines;
}

// ── Element-kind catalog (hydra-common spec §4.1) ───────────────────────────

/** The element classes the contract defines. */
export type ElementClass = "point" | "polyline" | "region" | "collection";

/** One element kind an engine models, as the engine describes it. */
/** What a kind does in the network (§4.3), as distinct from what it is
 * geometrically. Absent for kinds outside the flow network — a rain gage,
 * a curve, a control rule. */
export type ElementRole = "conveyance" | "boundary" | "control";

export interface ElementKindInfo {
  id: string;
  label: string;
  labelPlural: string;
  class: ElementClass;
  /** Absent where the kind plays no part in the flow network. */
  role?: ElementRole;
  /** One- or two-character glyph for dense UI. */
  badge: string;
  /** The heading this kind is listed under (§4.2.1), or absent for one
   * the engine places under none.
   *
   * The engine's own word rather than a class: a drainage engineer says
   * "nodes" and "links", not "points" and "polylines". Presentation
   * only — a list that ignores it is flat and correct. */
  group?: string;
  /** Whether elements of this kind may be created (§4.5.3). */
  creatable: boolean;
  /** What a new one would need that cannot be defaulted, in the
   * engine's own words. Present only when `creatable` is false. */
  notCreatableBecause?: string;
}

// Static per engine — a property of the domain, not of any model.
const kindCache = new Map<string, ElementKindInfo[]>();

async function getElementKinds(engine: string): Promise<ElementKindInfo[]> {
  const hit = kindCache.get(engine);
  if (hit) return hit;
  const kinds = await tryInvokeOr<ElementKindInfo[]>(
    "list_element_kinds",
    { engine },
    [],
  );
  kindCache.set(engine, kinds);
  // The engine's own badge letters become authoritative for every surface
  // that draws one, rather than each surface consulting a static copy.
  registerElementBadges(kinds);
  return kinds;
}

/** The element-kind catalog for `engine`; empty until it resolves. */
export function useElementKinds(
  engine: string | null | undefined,
): ElementKindInfo[] {
  const [kinds, setKinds] = useState<ElementKindInfo[]>(() =>
    engine ? (kindCache.get(engine) ?? []) : [],
  );
  useEffect(() => {
    if (!engine) {
      setKinds([]);
      return;
    }
    return fetchInto(getElementKinds(engine), setKinds);
  }, [engine]);
  return kinds;
}

/** One §4.4 property of an element kind, without any element's values. */
export interface ElementAttributeInfo {
  key: string;
  label: string;
  quantity?: GenericQuantity;
  /** Whether a write to this attribute may be offered (§4.5.1). */
  editable: boolean;
  /** The value's shape and bounds — what a create form needs to know
   * which control to draw, before any element exists to read. */
  kind: OptionKind;
  /** The kinds whose elements this attribute may name (§4.5.1.1).
   *
   * A list because a reference is not always to one kind — a drainage
   * subcatchment discharges to a conveyance node or to another
   * subcatchment. This interface said one kind until the create dialog
   * needed to offer them, which nothing had caught because nothing read
   * it. */
  references?: string[];
}

// Static per engine and kind, exactly as the kind catalog is.
const attributeCache = new Map<string, ElementAttributeInfo[]>();

/**
 * The declared property schema of one element kind.
 *
 * Known before any element is looked at, which is what lets a panel draw
 * its property rows while the values are still in flight instead of
 * appearing empty and then shoving everything below it down the panel.
 */
export function useElementAttributes(
  engine: string | null | undefined,
  kind: string | null | undefined,
): ElementAttributeInfo[] {
  const key = `${engine ?? ""}\u0000${kind ?? ""}`;
  const [attrs, setAttrs] = useState<ElementAttributeInfo[]>(
    () => attributeCache.get(key) ?? [],
  );
  useEffect(() => {
    if (!engine || !kind) {
      setAttrs([]);
      return;
    }
    const hit = attributeCache.get(key);
    if (hit) {
      setAttrs(hit);
      return;
    }
    return fetchInto(
      tryInvokeOr<ElementAttributeInfo[]>(
        "list_element_attributes",
        { engine, kind },
        [],
      ).then((list) => {
        // On the promise, not in the guarded apply: a cancelled fetch
        // still warms the cache for whoever asks next.
        attributeCache.set(key, list);
        return list;
      }),
      setAttrs,
    );
  }, [engine, kind, key]);
  return attrs;
}

/**
 * Heading for one element class, from the engine's own catalog: the kind's
 * plural label when the class holds exactly one kind ("Subcatchments"),
 * the class's generic name when it holds several ("Nodes").
 *
 * Precision when precision is available, generality when it is not — and
 * either way the words are the engine's, not this layer's. Derived from
 * the *declared* catalog rather than the loaded model, so a heading never
 * shifts because a particular network happens to contain one kind.
 */
export function elementClassHeading(
  kinds: ElementKindInfo[],
  cls: ElementClass,
  fallback: string,
): string {
  const inClass = kinds.filter((k) => k.class === cls);
  if (inClass.length === 1) return inClass[0].labelPlural;
  return fallback;
}
