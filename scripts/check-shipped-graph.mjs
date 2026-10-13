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

/** Crates that only exist for the Linux desktop backend, with the reason. */
const LINUX_ONLY = [
  { crate: 'glib', advisory: 'GHSA-wrw7-89jp-8q8g / RUSTSEC unsound (< 0.20.0)' },
  { crate: 'gtk', advisory: 'GTK 3 bindings, Linux backend only' },
  { crate: 'atk', advisory: 'accessibility bridge, Linux backend only' },
  { crate: 'gdk', advisory: 'GDK bindings, Linux backend only' },
  { crate: 'webkit2gtk', advisory: 'Linux webview backend' },
  { crate: 'libappindicator', advisory: 'Linux tray backend' },
];
const SHIPPED_TARGET = 'x86_64-pc-windows-msvc';
const UNSHIPPED_TARGET = 'x86_64-unknown-linux-gnu';

function tree(target, crate) {
  try {
    const output = execFileSync(
      'cargo',
      ['tree', '--locked', '--target', target, '-e', 'normal', '-i', crate],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
    );
    return { output, failed: false };
  } catch (error) {
    return { output: `${error.stdout ?? ''}${error.stderr ?? ''}`, failed: true };
  }
}

/** `cargo tree -i` exits 0 with "nothing to print" when the crate is absent. */
function present(result) {
  if (result.failed) return false;
  const text = result.output.trim();
  if (text.length === 0) return false;
  if (/nothing to print/i.test(text)) return false;
  return new RegExp(`^${'\\S'}+\\s`, 'm').test(text) || text.length > 0;
}

let failures = 0;
for (const { crate, advisory } of LINUX_ONLY) {
  const shipped = tree(SHIPPED_TARGET, crate);
  // Failure to inspect the Windows graph is not evidence of absence.
  // In particular, a target-specific Cargo resolution failure must block
  // release, even when Linux's negative control happens to pass.
  if (shipped.failed) {
    failures += 1;
    console.log(`FAIL cargo tree could not inspect the ${SHIPPED_TARGET} graph for ${crate}`);
    console.log(`     ${shipped.output.split('\\n').slice(0, 5).join('\\n     ')}`);
  } else if (present(shipped)) {
    failures += 1;
    console.log(`FAIL ${crate} is in the ${SHIPPED_TARGET} graph (${advisory})`);
    console.log(`     ${shipped.output.split('\n').slice(0, 5).join('\n     ')}`);
  } else {
    console.log(`OK   ${crate} absent from the shipped ${SHIPPED_TARGET} graph`);
  }
}

// The negative control: the same crates must be found for the Linux target.
// Without this, a broken cargo invocation would make every check above pass.
for (const { crate } of LINUX_ONLY) {
  const linux = tree(UNSHIPPED_TARGET, crate);
  if (!present(linux)) {
    failures += 1;
    console.log(`FAIL ${crate} is missing from the ${UNSHIPPED_TARGET} graph too - the check is not exercising anything`);
  }
}
console.log(
  failures === 0
    ? `\nshipped graph clean: no Linux-desktop crate is linked into ${SHIPPED_TARGET}`
    : `\n${failures} problem(s)`,
);
process.exitCode = failures === 0 ? 0 : 1;
