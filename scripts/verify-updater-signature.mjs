#!/usr/bin/env node
// Verify an updater artifact and its version using the public key shipped in KodeWork.
// Tauri stores a BASE64-encoded Minisign message in .msi.sig, not raw .minisig text.
import { readFileSync, writeFileSync, mkdtempSync, rmSync, realpathSync } from 'node:fs';
import { Buffer } from 'node:buffer';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);

export function decodeTauriBase64(value) {
  const encoded = String(value).trim();
  if (!encoded || !/^[A-Za-z0-9+/]+={0,2}$/.test(encoded)) {
    throw new Error('Invalid base64-encoded updater signature or public key');
  }
  const decoded = Buffer.from(encoded, 'base64');
  if (decoded.toString('base64') !== encoded) {
    throw new Error('Noncanonical base64 updater signature or public key');
  }
  return decoded.toString('utf8');
}

export function signedVersionFromTrustedComment(minisignText) {
  const lines = minisignText.replace(/\r\n/g, '\n').trimEnd().split('\n');
  if (lines.length !== 4 || !lines[0].startsWith('untrusted comment: ') ||
      !lines[2].startsWith('trusted comment: ')) {
    throw new Error('Invalid Minisign signature structure');
  }
  const fields = lines[2].slice('trusted comment: '.length).split('\t');
  const versions = fields.filter((field) => field.startsWith('version:'));
  if (versions.length !== 1) {
    throw new Error('Signature lacks a unique authenticated app version');
  }
  const signedVersion = versions[0].slice('version:'.length);
  if (!/^\d+\.\d+\.\d+$/.test(signedVersion)) {
    throw new Error('Signed updater version must be stable X.Y.Z');
  }
  return signedVersion;
}

export function requireMatchingVersion(minisignText, manifestVersion) {
  if (!/^\d+\.\d+\.\d+$/.test(String(manifestVersion))) {
    throw new Error('Announced updater version must be stable X.Y.Z');
  }
  const signedVersion = signedVersionFromTrustedComment(minisignText);
  if (signedVersion !== manifestVersion) {
    throw new Error('Authenticated signed updater version does not match the announced version');
  }
}

export function verifyUpdaterArtifact(artifactPath, signaturePath, version) {
  if (!artifactPath || !signaturePath || !version) {
    throw new Error('Usage: node scripts/verify-updater-signature.mjs <MSI> <MSI.sig> <X.Y.Z>');
  }
  // The original encoded signature and public key are untrusted until Minisign
  // successfully verifies the MSI bytes and the global signature of the comment.
  const decodedSignature = decodeTauriBase64(readFileSync(signaturePath, 'utf8'));
  const configPath = resolve(dirname(scriptPath), '../src-tauri/tauri.conf.json');
  const config = JSON.parse(readFileSync(configPath, 'utf8'));
  if (config.plugins?.updater?.requireSignedVersion !== true) {
    throw new Error('Updater must enable requireSignedVersion before release');
  }
  const decodedPubkey = decodeTauriBase64(config.plugins.updater.pubkey);
  const temp = mkdtempSync(join(tmpdir(), 'kodework-updater-verify-'));
  try {
    const publicKey = join(temp, 'updater.pub');
    const signature = join(temp, 'updater.minisig');
    writeFileSync(publicKey, decodedPubkey, { mode: 0o600 });
    writeFileSync(signature, decodedSignature, { mode: 0o600 });
    const verified = spawnSync('minisign',
      ['-Vm', resolve(artifactPath), '-x', signature, '-p', publicKey, '-q'],
      { encoding: 'utf8', windowsHide: true, timeout: 120000 });
    if (verified.error || verified.status !== 0) {
      throw new Error('Minisign cryptographic verification of MSI/signature failed');
    }
    // Do not trust the version text until the global Minisign signature has passed.
    requireMatchingVersion(decodedSignature, version);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

// Decide "am I the program that was started?" by comparing *resolved real
// paths*. A plain string comparison against `import.meta.url` silently skips
// this whole block — including the exit code below — whenever the same file is
// reached through a junction, symlink, 8.3 short name, differently-cased path
// or a shim, and the callers only inspect the exit status. Skipping must never
// be able to look like success, so an unresolvable entry path is treated as a
// direct invocation and allowed to fail loudly instead.
function isDirectInvocation() {
  if (!process.argv[1]) return false;
  try {
    return realpathSync(resolve(process.argv[1])) === realpathSync(scriptPath);
  } catch {
    return true;
  }
}

if (isDirectInvocation()) {
  try {
    verifyUpdaterArtifact(...process.argv.slice(2));
    console.log('Updater artifact signature and authenticated version verified.');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
