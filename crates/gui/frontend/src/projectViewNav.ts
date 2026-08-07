/**
 * Asking for a project view means two different things.
 *
 * From a nav button it means "go there", and asking for the view you are
 * already on means "collapse the rail" — the second press of a tab closes
 * the panel it opened. That is good behaviour for a tab and a trap for
 * everything else: a command or a shortcut that wants the canvas so it can
 * act on it says `setProjectView("canvas")` and, if the user was already
 * looking at the canvas, silently collapses their network list instead.
 *
 * It has caught callers before — `focusInEditor` carries a comment about
 * not toggling the rail "the way `setProjectView('editor')` would" — and it
 * caught the element-finder shortcut and the palette's own "Find an element
 * on canvas…" command, both of which navigate before doing their real work.
 *
 * So the rule has a name, and a caller that only wants to be somewhere asks
 * for that instead of re-deriving the condition.
 */

/**
 * Whether this request is a reselect — the same view on the same page — and
 * therefore means the rail rather than navigation.
 */
export function reselectsCurrentView(
  page: string,
  currentView: string,
  requestedView: string,
): boolean {
  return page === "project" && currentView === requestedView;
}
