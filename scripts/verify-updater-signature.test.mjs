import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  decodeTauriBase64,
  signedVersionFromTrustedComment,
  requireMatchingVersion,
} from './verify-updater-signature.mjs';

// Synthetic signature TEXT only. Cryptographic verification is performed by
// the release gate with Minisign + the real public key and MSI bytes.
const mockSig = (comment) => [
  'untrusted comment: signature from tauri secret key',
  'RUTESTSIGNATURE',
  'trusted comment: ' + comment,
  'GLOBALTESTSIGNATURE',
].join('\n') + '\n';

test('Tauri .sig is canonical base64 containing a Minisign message', () => {
  const value = mockSig('timestamp:1\tfile:app.msi\tversion:0.2.4');
  assert.equal(decodeTauriBase64(Buffer.from(value).toString('base64')), value);
  assert.throws(() => decodeTauriBase64('not base64!!!'), /Invalid base64/);
  assert.throws(() => decodeTauriBase64('Zm8'), /Noncanonical base64/);
});

test('accept only a unique signed version that matches the announced version', () => {
  const good = mockSig('timestamp:1\tfile:app.msi\tversion:0.2.4');
  assert.equal(signedVersionFromTrustedComment(good), '0.2.4');
  assert.doesNotThrow(() => requireMatchingVersion(good, '0.2.4'));
  assert.throws(() => requireMatchingVersion(good, '0.2.5'), /does not match/);
  assert.throws(() => requireMatchingVersion(good, 'v0.2.4'), /stable/);
});

test('reject unversioned, duplicated, malformed, or malformed-structure signatures', () => {
  assert.throws(() => signedVersionFromTrustedComment(mockSig('timestamp:1\tfile:app.msi')), /unique/);
  assert.throws(() => signedVersionFromTrustedComment(mockSig('version:0.2.4\tversion:0.2.5')), /unique/);
  assert.throws(() => signedVersionFromTrustedComment(mockSig('version:not-semver')), /stable/);
  assert.throws(() => signedVersionFromTrustedComment('trusted comment: version:0.2.4'), /structure/);
});

// The release gate inspects only the exit status of this script, so the process
// contract itself is what must be tested: every rejected input has to leave a
// non-zero status behind, and a silent no-op must never be reachable.
const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const verifierPath = join(scriptsDirectory, 'verify-updater-signature.mjs');
const runVerifier = (args) => spawnSync(process.execPath, [verifierPath, ...args], { encoding: 'utf8' });

function withTempDirectory(run) {
  const directory = mkdtempSync(join(tmpdir(), 'kodework-verifier-test-'));
  try {
    return run(directory);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test('the verifier fails closed on missing arguments', () => {
  const result = runVerifier([]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Usage/);
});

test('the verifier fails closed on a malformed signature file', () => {
  withTempDirectory((directory) => {
    const artifact = join(directory, 'kodework.msi');
    const signature = join(directory, 'kodework.msi.sig');
    writeFileSync(artifact, 'not really an MSI');
    writeFileSync(signature, 'not base64!!!');
    const result = runVerifier([artifact, signature, '0.2.4']);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /base64/);
  });
});

test('the verifier fails closed when the artifact is absent', () => {
  withTempDirectory((directory) => {
    const signature = join(directory, 'kodework.msi.sig');
    writeFileSync(signature, Buffer.from(mockSig('version:0.2.4')).toString('base64'));
    const result = runVerifier([join(directory, 'missing.msi'), signature, '0.2.4']);
    assert.notEqual(result.status, 0);
  });
});

test('the verifier fails closed when the artifact is not signed by the shipped key', () => {
  withTempDirectory((directory) => {
    const artifact = join(directory, 'kodework.msi');
    const signature = join(directory, 'kodework.msi.sig');
    writeFileSync(artifact, 'not really an MSI');
    writeFileSync(signature, Buffer.from(mockSig('timestamp:1\tversion:0.2.4')).toString('base64'));
    const result = runVerifier([artifact, signature, '0.2.4']);
    // Either Minisign is missing (spawn error) or it rejects the bogus signature.
    assert.notEqual(result.status, 0);
    assert.notEqual(result.stderr.trim(), '');
    assert.match(result.stderr, /Minisign|base64|structure|version/);
  });
});

test('reachability through a path alias cannot turn verification into a silent pass', () => {
  withTempDirectory((directory) => {
    const alias = join(directory, 'aliased-scripts');
    try {
      symlinkSync(scriptsDirectory, alias, process.platform === 'win32' ? 'junction' : 'dir');
    } catch {
      return; // Symlinks unavailable in this environment; the guard is still covered above.
    }
    const aliasedVerifier = join(alias, 'verify-updater-signature.mjs');
    const result = spawnSync(process.execPath, [aliasedVerifier], { encoding: 'utf8' });
    // Regression guard: this used to exit 0 with no output, which every caller
    // read as "signature verified".
    assert.notEqual(result.status, 0, 'an aliased invocation must not report success');
    assert.match(result.stderr, /Usage/);
  });
});
