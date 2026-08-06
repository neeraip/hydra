/**
 * Which variable the canvas is showing.
 *
 * There used to be two answers to that. The legend's picker held one, as a
 * catalog variable id; the canvas held another, as a typed name it paints
 * and that the hover chip and the element inspector read. A select handler
 * wrote both, so they normally agreed — and every path that wrote only one
 * left them naming different variables with nothing on screen to say which
 * was right. Once split they stayed split, because the split was persisted
 * and nothing reconciled it.
 *
 * So there is one store now — the legend's selection — and these turn its
 * id into the typed name. An id the canvas does not recognise is not an
 * error: another engine's catalog has its own ids, and its canvas takes a
 * different path entirely.
 */

import type { LinkVariable, NodeVariable } from "./types";

/**
 * Every variable, as values rather than types.
 *
 * Fully-keyed records so the compiler rejects the list the moment a variable
 * joins either union. An array literal would go stale in silence.
 */
const EVERY_NODE_VARIABLE: Record<NodeVariable, true> = {
  pressure: true,
  head: true,
  demand: true,
  quality: true,
};

const EVERY_LINK_VARIABLE: Record<LinkVariable, true> = {
  flow: true,
  velocity: true,
  status: true,
  headloss: true,
  quality: true,
};

export const NODE_VARIABLES = Object.keys(
  EVERY_NODE_VARIABLE,
) as readonly NodeVariable[];

export const LINK_VARIABLES = Object.keys(
  EVERY_LINK_VARIABLE,
) as readonly LinkVariable[];

/** The catalog id as a node variable, or `null` if it is not one. */
export function asNodeVariable(id: string): NodeVariable | null {
  return (NODE_VARIABLES as readonly string[]).includes(id)
    ? (id as NodeVariable)
    : null;
}

/** The catalog id as a link variable, or `null` if it is not one. */
export function asLinkVariable(id: string): LinkVariable | null {
  return (LINK_VARIABLES as readonly string[]).includes(id)
    ? (id as LinkVariable)
    : null;
}

/**
 * What to paint nodes with, given what the legend has selected.
 *
 * The fallback covers an empty selection — before a catalog has been chosen
 * from — and an id belonging to some other engine.
 */
export function nodeVariableFor(
  selectedId: string,
  fallback: NodeVariable,
): NodeVariable {
  return asNodeVariable(selectedId) ?? fallback;
}

/** And links. */
export function linkVariableFor(
  selectedId: string,
  fallback: LinkVariable,
): LinkVariable {
  return asLinkVariable(selectedId) ?? fallback;
}
