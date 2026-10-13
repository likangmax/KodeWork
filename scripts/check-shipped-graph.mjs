// Proves that the crates carrying a Linux-desktop advisory are absent from the
// dependency graph we actually ship, and present in the one we do not.
//
// Background: GHSA-wrw7-89jp-8q8g (glib, unsoundness in `VariantStrIter`) is
// reported against `Cargo.lock`, which is target-independent. The vulnerable
// range is `>= 0.15.0, < 0.20.0` and the fix is glib 0.20.0, but the only path to
// glib is the GTK 3 stack (gtk 0.18 -> atk/cairo-rs/gdk -> glib 0.18) pulled in
// by Tauri's Linux backend. gtk 0.18 requires glib ^0.18, so no version bump
// inside this workspace can reach the fix; only upstream moving Tauri to gtk-rs
// 0.20 can. KodeWork ships a Windows MSI only, so the crate is not linked into
// anything we distribute - which is the claim this script enforces mechanically
// instead of stating it in prose.
//
// Usage: node scripts/check-shipped-graph.mjs
import { execFileSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Crates that only exist for the Linux desktop backend, with the reason. */
const LINUX_ONLY = [
  { crate: 'glib', advisory: 'GHSA-wrw7-89jp-8q8g / RUSTSEC unsound (< 0.20.0)' },
  { crate: 'gtk', advisory: 'GTK 3 bindings, Linux backend only' },
  { crate: 'atk', advisory: 'accessibility bridge, Linux backend only' },
  { crate: 'gdk', advisory: 'GDK bindings, Linux backend only' },
  { crate: 'webkit2gtk', advisory: 'Linux webview backend' },
  { crate: 'libappindicator', advisory: 'Linux tray backend' },
];
export const SHIPPED_TARGET = 'x86_64-pc-windows-msvc';
export const UNSHIPPED_TARGET = 'x86_64-unknown-linux-gnu';

/** A cargo invocation that never finishes must not look like a clean graph. */
const CARGO_TIMEOUT_MS = 180_000;

export function tree(target, crate) {
  try {
    const output = execFileSync(
      'cargo',
      ['tree', '--locked', '--target', target, '-e', 'normal', '-i', crate],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: CARGO_TIMEOUT_MS },
    );
    return { output, failed: false };
  } catch (error) {
    return { output: `${error.stdout ?? ''}${error.stderr ?? ''}`, failed: true };
  }
}

/**
 * Classifies one `cargo tree -i` result.
 *
 * `cargo tree -i` exits 0 and reports "nothing to print" when the crate is
 * absent, so absence is decided by the output rather than by the exit status. A
 * query that could not run at all is `error`, never `absent`: treating a broken
 * invocation as absence is how a dependency check silently stops checking.
 */
export function classifyTree(result) {
  if (result.failed) return 'error';
  const text = String(result.output ?? '');
  if (text.trim().length === 0) return 'absent';
  if (/nothing to print/i.test(text)) return 'absent';
  // A real reverse-dependency tree starts with the crate itself and its version.
  // Anything else is treated as an error rather than as evidence of absence: this
  // check exists to fail closed.
  if (/^[\w.+@/-]+ v\d/m.test(text)) return 'present';
  return 'error';
}

/**
 * The decision for one crate: a finding string when the shipped graph is not
 * clean for it, otherwise null.
 */
export function evaluateCrate(crate, advisory, shipped, linux) {
  const shippedKind = classifyTree(shipped);
  if (shippedKind === 'error') {
    return `cargo tree could not inspect the ${SHIPPED_TARGET} graph for ${crate}:\n     ${firstLines(shipped.output)}`;
  }
  if (shippedKind === 'present') {
    return `${crate} is in the ${SHIPPED_TARGET} graph (${advisory}):\n     ${firstLines(shipped.output)}`;
  }
  // Negative control: without it, a broken cargo invocation would make every
  // "absent" verdict above vacuous.
  if (classifyTree(linux) !== 'present') {
    return `${crate} is missing from the ${UNSHIPPED_TARGET} graph too - the check is not exercising anything`;
  }
  return null;
}

function firstLines(output) {
  return String(output ?? '').split('\n').slice(0, 5).join('\n     ');
}

function run() {
  let failures = 0;
  for (const { crate, advisory } of LINUX_ONLY) {
    const finding = evaluateCrate(crate, advisory, tree(SHIPPED_TARGET, crate), tree(UNSHIPPED_TARGET, crate));
    if (finding) {
      failures += 1;
      console.log(`FAIL ${finding}`);
    } else {
      console.log(`OK   ${crate} absent from the shipped ${SHIPPED_TARGET} graph`);
    }
  }
  console.log(
    failures === 0
      ? `\nshipped graph clean: no Linux-desktop crate is linked into ${SHIPPED_TARGET}`
      : `\n${failures} problem(s)`,
  );
  process.exitCode = failures === 0 ? 0 : 1;
}

// Only run when invoked as the program: importing this module for its classifier
// must not shell out to cargo.
function isDirectInvocation() {
  if (!process.argv[1]) return false;
  try {
    return realpathSync(resolve(process.argv[1])) === realpathSync(fileURLToPath(import.meta.url));
  } catch {
    return true;
  }
}

if (isDirectInvocation()) run();
