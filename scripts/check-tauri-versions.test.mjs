import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  compareTauriVersions,
  majorMinor,
  parseCargoLockVersions,
  parseNpmLockVersions,
} from './check-tauri-versions.mjs';

const cargoLock = (entries) =>
  entries
    .map(([name, version]) => `[[package]]\nname = "${name}"\nversion = "${version}"\n`)
    .join('\n');

const packageLock = (entries) => JSON.stringify({ packages: Object.fromEntries(entries) }, null, 2);

describe('parseCargoLockVersions', () => {
  it('reads crate versions and ignores unrelated blocks', () => {
    const versions = parseCargoLockVersions(
      cargoLock([
        ['tauri', '2.12.3'],
        ['tauri-plugin-updater', '2.13.1'],
        ['serde', '1.0.229'],
      ]),
    );
    assert.equal(versions.get('tauri'), '2.12.3');
    assert.equal(versions.get('tauri-plugin-updater'), '2.13.1');
    assert.equal(versions.get('serde'), '1.0.229');
  });
});

describe('parseNpmLockVersions', () => {
  it('reads top-level installs and ignores nested duplicates', () => {
    const versions = parseNpmLockVersions(
      packageLock([
        ['node_modules/@tauri-apps/api', { version: '2.12.3' }],
        ['node_modules/foo/node_modules/@tauri-apps/api', { version: '1.0.0' }],
        ['node_modules/@tauri-apps/cli', { version: '2.12.2' }],
      ]),
    );
    assert.equal(versions.get('@tauri-apps/api'), '2.12.3');
    assert.equal(versions.get('@tauri-apps/cli'), '2.12.2');
  });

  it('skips entries without a version', () => {
    const versions = parseNpmLockVersions(packageLock([['node_modules/linked', { link: true }]]));
    assert.equal(versions.size, 0);
  });
});

describe('majorMinor', () => {
  it('keeps only the major/minor pair and rejects junk', () => {
    assert.equal(majorMinor('2.12.3'), '2.12');
    assert.equal(majorMinor('2.12.3-beta.1'), '2.12');
    assert.equal(majorMinor('2'), null);
  });
});

describe('compareTauriVersions', () => {
  const npm = (entries) => parseNpmLockVersions(packageLock(entries));
  const cargo = (entries) => parseCargoLockVersions(cargoLock(entries));

  it('accepts a graph whose crate and npm packages share major/minor', () => {
    const mismatches = compareTauriVersions(
      cargo([
        ['tauri', '2.12.3'],
        ['tauri-plugin-updater', '2.13.1'],
        ['tauri-plugin-dialog', '2.7.3'],
      ]),
      npm([
        ['node_modules/@tauri-apps/api', { version: '2.12.3' }],
        ['node_modules/@tauri-apps/plugin-updater', { version: '2.13.3' }],
        ['node_modules/@tauri-apps/plugin-dialog', { version: '2.7.3' }],
      ]),
    );
    assert.deepEqual(mismatches, []);
  });

  it('reports the exact regression this check exists for', () => {
    // The real 6a3b81f state: Rust crates were bumped, the npm ranges were not.
    const mismatches = compareTauriVersions(
      cargo([
        ['tauri', '2.12.3'],
        ['tauri-plugin-updater', '2.13.1'],
      ]),
      npm([
        ['node_modules/@tauri-apps/api', { version: '2.11.1' }],
        ['node_modules/@tauri-apps/plugin-updater', { version: '2.11.0' }],
      ]),
    );
    assert.equal(mismatches.length, 2);
    assert.deepEqual(
      mismatches.map((entry) => [entry.crate, entry.npmName]),
      [
        ['tauri', '@tauri-apps/api'],
        ['tauri-plugin-updater', '@tauri-apps/plugin-updater'],
      ],
    );
  });

  it('tolerates a patch-level difference inside the same major/minor', () => {
    const mismatches = compareTauriVersions(
      cargo([['tauri-plugin-updater', '2.13.1']]),
      npm([['node_modules/@tauri-apps/plugin-updater', { version: '2.13.3' }]]),
    );
    assert.deepEqual(mismatches, []);
  });

  it('flags an installed npm plugin that has no crate counterpart', () => {
    const mismatches = compareTauriVersions(
      cargo([['tauri', '2.12.3']]),
      npm([['node_modules/@tauri-apps/plugin-nope', { version: '1.0.0' }]]),
    );
    assert.equal(mismatches.length, 1);
    assert.match(mismatches[0].reason, /not in Cargo\.lock/);
  });

  it('ignores a crate whose npm package is not installed', () => {
    const mismatches = compareTauriVersions(cargo([['tauri', '2.12.3']]), npm([]));
    assert.deepEqual(mismatches, []);
  });
});
