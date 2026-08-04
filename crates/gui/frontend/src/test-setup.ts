/**
 * Vitest setup for DOM tests.
 *
 * Unmounts anything a test rendered once it finishes. Without this the
 * document accumulates every render in the file, and queries start finding
 * the previous test's elements — which surfaces as "found multiple
 * elements" at best, and as a test asserting against a stale tree at worst.
 */
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(cleanup);
