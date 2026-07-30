// ─────────────────────────────────────────────────────────────────────────────
// INP identifier format rules.
//
// Mirror of the backend `validate_inp_id` (crates/gui/src/commands/mutations.rs).
// The backend is the authoritative gate — every id-accepting command runs it —
// but a command's rejection arrives as a toast after the fact, so the dialogs
// run the same rules inline to keep the feedback next to the field.
//
// These are not stylistic rules. INP is a whitespace-delimited format, so an id
// holding a space is written as two fields and cannot be read back: the file
// the app just wrote fails to parse the next time it is opened. `;` starts a
// comment and quotes confuse tokenisation the same way. Collisions are not
// checked here — those need the network, and the backend reports them.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Validate an id's *format*, ignoring collisions.
 *
 * @returns an error message, or `null` when the format is acceptable.
 */
export function inpIdError(raw: string): string | null {
  const t = raw.trim();
  if (!t) return "ID must not be empty";
  if (/\s/.test(t)) return "ID must not contain spaces";
  if (/[;"']/.test(t)) return "ID must not contain “ ; ” or quotes";
  return null;
}
