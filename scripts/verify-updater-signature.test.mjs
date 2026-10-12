import test from 'node:test';
import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
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
