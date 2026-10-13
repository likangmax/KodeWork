// Fail before `tauri build` does: the Tauri CLI refuses to build when a Rust
// crate and its npm counterpart are not on the same major/minor release, and
// that failure only surfaces on the release path because CI never bundles the
// desktop app. This check is cheap, needs no Rust toolchain, and runs in the
// frontend CI job.
//
// Rule enforced (matches the CLI's own "version mismatched Tauri packages"
// preflight): `tauri` <-> `@tauri-apps/api`, and `tauri-plugin-<name>` <->
// `@tauri-apps/plugin-<name>`. Versions are read from the lockfiles so the
// check covers exactly what `npm ci` and `cargo --locked` will resolve.
import { readFileSync, realpathSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function parseCargoLockVersions(text) {
  const versions = new Map();
  const blocks = String(text).split(/^\[\[package\]\]$/m).slice(1);
  for (const block of blocks) {
    const name = block.match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1];
    const version = block.match(/^\s*version\s*=\s*"([^"]+)"/m)?.[1];
    if (name && version) versions.set(name, version);
  }
  return versions;
}

export function parseNpmLockVersions(text) {
  const parsed = JSON.parse(text);
  const versions = new Map();
  for (const [entryPath, entry] of Object.entries(parsed.packages ?? {})) {
    if (!entryPath.startsWith('node_modules/') || !entry?.version) continue;
    const name = entryPath.slice('node_modules/'.length);
    // Nested installs (a/b/node_modules/c) must not shadow the top-level one.
    if (name.includes('/node_modules/')) continue;
    versions.set(name, entry.version);
  }
  return versions;
}

export function majorMinor(version) {
  const match = String(version).trim().match(/^(\d+)\.(\d+)\./);
  return match ? `${match[1]}.${match[2]}` : null;
}

/** Returns the list of mismatched crate/npm pairs; empty means the graph agrees. */
export function compareTauriVersions(cargoVersions, npmVersions) {
  const pairs = [];
  for (const [crate, npmName] of [
    ['tauri', '@tauri-apps/api'],
    ...npmVersionsToPluginCrates(npmVersions),
  ]) {
    const crateVersion = cargoVersions.get(crate);
    const npmVersion = npmVersions.get(npmName);
    pairs.push({ crate, crateVersion, npmName, npmVersion });
  }

  const mismatches = [];
  for (const pair of pairs) {
    const { crate, crateVersion, npmVersion } = pair;
    if (!npmVersion) continue; // An npm-side package that is not installed cannot skew the build.
    if (!crateVersion) {
      mismatches.push({ ...pair, reason: `${crate} is not in Cargo.lock` });
      continue;
    }
    const crateMM = majorMinor(crateVersion);
    const npmMM = majorMinor(npmVersion);
    if (!crateMM || !npmMM) {
      mismatches.push({ ...pair, reason: 'unparseable version' });
      continue;
    }
    if (crateMM !== npmMM) {
      mismatches.push({ ...pair, reason: `major/minor differ (${crateMM} vs ${npmMM})` });
    }
  }
  return mismatches;
}

function npmVersionsToPluginCrates(npmVersions) {
  const pairs = [];
  for (const npmName of npmVersions.keys()) {
    const pluginName = npmName.match(/^@tauri-apps\/plugin-(.+)$/)?.[1];
    if (pluginName) pairs.push([`tauri-plugin-${pluginName}`, npmName]);
  }
  return pairs.sort((a, b) => a[0].localeCompare(b[0]));
}

function main(projectRoot) {
  const cargoVersions = parseCargoLockVersions(readFileSync(join(projectRoot, 'Cargo.lock'), 'utf8'));
  const npmVersions = parseNpmLockVersions(readFileSync(join(projectRoot, 'package-lock.json'), 'utf8'));
  const mismatches = compareTauriVersions(cargoVersions, npmVersions);

  if (mismatches.length > 0) {
    console.error('Tauri crate/npm version mismatch; `tauri build` will refuse to run:');
    for (const mismatch of mismatches) {
      console.error(`- ${mismatch.crate} (${mismatch.crateVersion ?? 'absent'}) : ${mismatch.npmName} (${mismatch.npmVersion ?? 'absent'}) — ${mismatch.reason}`);
    }
    console.error('Align the npm range in package.json with the locked Rust crate, then refresh package-lock.json with `npm install`.');
    process.exitCode = 1;
    return;
  }

  const checked = [...npmVersions.keys()].filter((name) => name === '@tauri-apps/api' || name.startsWith('@tauri-apps/plugin-'));
  console.log(`Tauri crate/npm versions agree across ${checked.length} package(s).`);
}

const scriptPath = fileURLToPath(import.meta.url);
// Compare resolved real paths, never the raw argv string: on Windows a junction,
// symlink, short (8.3) name or differently-cased path makes a naive comparison
// fail and would silently turn this check into a no-op that exits 0.
if (process.argv[1] && realpathSync(resolve(process.argv[1])) === realpathSync(scriptPath)) {
  main(dirname(dirname(scriptPath)));
}
