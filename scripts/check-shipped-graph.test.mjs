// The shipped-graph check decides "this advisory crate is not linked into the
// artifact we ship" from the output of `cargo tree`. These tests pin the
// classification, because the failure mode of getting it wrong is a dependency
// check that silently stops checking.
import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyTree, evaluateCrate, SHIPPED_TARGET, UNSHIPPED_TARGET } from './check-shipped-graph.mjs';

const TREE_FOR_A_CRATE = [
  'glib v0.18.5',
  '├── atk v0.18.2',
  '│   └── gtk v0.18.2',
  '└── gio v0.18.4',
].join('\n');

test('an absent crate is recognised from empty output and from the cargo notice', () => {
  // `cargo tree -i` exits 0 in both of these cases.
  assert.equal(classifyTree({ output: '', failed: false }), 'absent');
  assert.equal(classifyTree({ output: 'warning: nothing to print.\n', failed: false }), 'absent');
});

test('a crate that is present is recognised from its tree', () => {
  assert.equal(classifyTree({ output: TREE_FOR_A_CRATE, failed: false }), 'present');
});

test('a query that could not run is an error, never an absent crate', () => {
  // This is the regression: a failed inspection used to read as "not present",
  // which let the check pass without inspecting anything.
  assert.equal(classifyTree({ output: 'error: no such command: tree', failed: true }), 'error');
  assert.equal(classifyTree({ output: '', failed: true }), 'error');
});

test('an unclassifiable output is an error rather than an absent crate', () => {
  assert.equal(classifyTree({ output: 'something entirely unexpected\n', failed: false }), 'error');
});

test('a failed shipped-graph inspection fails the check even when Linux resolves', () => {
  const finding = evaluateCrate(
    'glib',
    'GHSA-wrw7-89jp-8q8g',
    { output: 'error: failed to select a version', failed: true },
    { output: TREE_FOR_A_CRATE, failed: false },
  );
  assert.match(finding ?? '', new RegExp(`could not inspect the ${SHIPPED_TARGET} graph`));
});

test('a present advisory crate fails the check', () => {
  const finding = evaluateCrate('glib', 'GHSA-wrw7-89jp-8q8g', { output: TREE_FOR_A_CRATE, failed: false }, { output: TREE_FOR_A_CRATE, failed: false });
  assert.match(finding ?? '', new RegExp(`is in the ${SHIPPED_TARGET} graph`));
});

test('an absent crate passes only when the Linux control still resolves', () => {
  const clean = evaluateCrate('glib', 'advisory', { output: '', failed: false }, { output: TREE_FOR_A_CRATE, failed: false });
  assert.equal(clean, null);
  // A broken cargo invocation must not make every "absent" verdict vacuous.
  const vacuous = evaluateCrate('glib', 'advisory', { output: '', failed: false }, { output: '', failed: false });
  assert.match(vacuous ?? '', new RegExp(`missing from the ${UNSHIPPED_TARGET} graph`));
});
