/**
 * Setup for the layout tests.
 *
 * Loads the app's own stylesheet, because layout is a product of the
 * cascade and not of inline styles alone: the global `box-sizing:
 * border-box` reset, the rem-based type scale and the reduced-motion rules
 * all change what a box measures. Without it a column declared 680px wide
 * measured 776 — its padding added on the outside, exactly as it would not
 * in the running app.
 */
import "./app.css";
