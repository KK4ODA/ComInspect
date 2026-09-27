#!/usr/bin/env node
// Validates the updater manifest (latest.json) produced by tauri-action.
//
//   node scripts/check-updater-manifest.mjs latest.json 1.2.3 [--config src-tauri/tauri.conf.json]
//     Fails unless the manifest announces 1.2.3 and has a signed download for
//     every required platform. With --config, every signature must also have
//     been made by the key whose public half is in that config (the key
//     installed apps trust) and be bound to version 1.2.3, which is what the
//     app's updater checks with requireSignedVersion. A mismatch there would
//     make every installed copy refuse the update.
//
//   node scripts/check-updater-manifest.mjs --newer candidate.json current.json
//     Exits 0 when candidate.json announces a newer version than
//     current.json (or current.json does not exist), 1 otherwise.
import { createPublicKey, verify } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';

const REQUIRED_PLATFORMS = ['windows-x86_64', 'darwin-aarch64', 'darwin-x86_64', 'linux-x86_64'];
// Installations from these packages update only from a package of the same
// kind; without an entry the app asks those users to download it manually.
const PACKAGE_PLATFORMS = ['windows-x86_64-msi', 'linux-x86_64-deb', 'linux-x86_64-rpm'];

function parseVersion(v) {
  const [core, pre] = String(v).replace(/^v/, '').split('-', 2);
  const nums = core.split('.').map((n) => Number.parseInt(n, 10) || 0);
  return { nums, pre: pre ?? null };
}

/** Semver comparison (pre-release identifiers compared as strings/numbers). */
export function compareVersions(a, b) {
  const x = parseVersion(a);
  const y = parseVersion(b);
  for (let i = 0; i < 3; i++) {
    if ((x.nums[i] ?? 0) !== (y.nums[i] ?? 0)) return (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
  }
  if (x.pre === y.pre) return 0;
  if (x.pre === null) return 1;
  if (y.pre === null) return -1;
  const xa = x.pre.split('.');
  const ya = y.pre.split('.');
  for (let i = 0; i < Math.max(xa.length, ya.length); i++) {
    if (xa[i] === undefined) return -1;
    if (ya[i] === undefined) return 1;
    const xn = Number(xa[i]);
    const yn = Number(ya[i]);
    if (!Number.isNaN(xn) && !Number.isNaN(yn) && xn !== yn) return xn - yn;
    if (xa[i] !== ya[i]) return xa[i] < ya[i] ? -1 : 1;
  }
  return 0;
}

// --- minisign, as used by the Tauri updater ------------------------------
// Both keys and signatures are base64 of a minisign text file.

function minisignLines(base64) {
  return Buffer.from(String(base64).trim(), 'base64')
    .toString('utf8')
    .split('\n')
    .map((line) => line.replace(/\r$/, ''))
    .filter((line) => line.length > 0);
}

/** Key IDs are shown like minisign does: the 8 bytes little-endian, in hex. */
const keyIdText = (bytes) => Buffer.from(bytes).reverse().toString('hex').toUpperCase();

export function parsePublicKey(base64) {
  const lines = minisignLines(base64);
  const raw = Buffer.from(lines[1] ?? '', 'base64');
  if (raw.length !== 42 || raw.subarray(0, 2).toString() !== 'Ed') {
    throw new Error('plugins.updater.pubkey is not a minisign public key');
  }
  const key = createPublicKey({
    key: { kty: 'OKP', crv: 'Ed25519', x: raw.subarray(10).toString('base64url') },
    format: 'jwk',
  });
  return { keyId: keyIdText(raw.subarray(2, 10)), key };
}

export function parseSignature(base64) {
  const lines = minisignLines(base64);
  const raw = Buffer.from(lines[1] ?? '', 'base64');
  const trustedPrefix = 'trusted comment: ';
  if (raw.length !== 74 || !lines[2]?.startsWith(trustedPrefix) || !lines[3]) {
    throw new Error('not a minisign signature');
  }
  return {
    keyId: keyIdText(raw.subarray(2, 10)),
    signature: raw.subarray(10),
    trustedComment: lines[2].slice(trustedPrefix.length),
    globalSignature: Buffer.from(lines[3], 'base64'),
  };
}

/**
 * Checks a signature from the manifest against the trusted public key without
 * downloading the file: the key ID must match, and the global signature (over
 * the file signature and the trusted comment) must verify, which proves the
 * trusted comment, including its version, came from that key.
 */
export function checkSignature(signatureBase64, publicKey, expectedVersion) {
  let sig;
  try {
    sig = parseSignature(signatureBase64);
  } catch {
    return 'signature is not a valid minisign signature';
  }
  if (sig.keyId !== publicKey.keyId) {
    return `signed with key ${sig.keyId}, but installed apps trust key ${publicKey.keyId} (plugins.updater.pubkey); check the TAURI_SIGNING_PRIVATE_KEY secret`;
  }
  const signed = Buffer.concat([sig.signature, Buffer.from(sig.trustedComment, 'utf8')]);
  if (!verify(null, signed, publicKey.key, sig.globalSignature)) {
    return 'signature does not verify against plugins.updater.pubkey';
  }
  const version = sig.trustedComment
    .split('\t')
    .find((field) => field.startsWith('version:'))
    ?.slice('version:'.length);
  if (!version) {
    return 'signature does not record the version it was signed for (requireSignedVersion rejects it)';
  }
  if (compareVersions(version, expectedVersion) !== 0) {
    return `signature is for version ${version}, not ${expectedVersion}`;
  }
  return null;
}

// --- command line -----------------------------------------------------------

function load(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function checkManifest(file, expected, configPath) {
  const manifest = load(file);
  const problems = [];
  if (expected && manifest.version.replace(/^v/, '') !== expected) {
    problems.push(`manifest announces ${manifest.version}, expected ${expected}`);
  }
  for (const platform of REQUIRED_PLATFORMS) {
    const entry = manifest.platforms?.[platform];
    if (!entry) problems.push(`missing platform ${platform}`);
    else {
      if (!entry.url?.startsWith('https://')) problems.push(`${platform}: invalid url`);
      if (!entry.signature || entry.signature.length < 40) problems.push(`${platform}: missing signature`);
    }
  }
  if (configPath) {
    const pubkey = load(configPath).plugins?.updater?.pubkey;
    let publicKey = null;
    try {
      publicKey = parsePublicKey(pubkey);
    } catch (e) {
      problems.push(`${configPath}: ${e.message}`);
    }
    if (publicKey) {
      for (const [platform, entry] of Object.entries(manifest.platforms ?? {})) {
        if (!entry?.signature) continue;
        const problem = checkSignature(entry.signature, publicKey, expected ?? manifest.version);
        if (problem) problems.push(`${platform}: ${problem}`);
      }
    }
  }
  const missingPackages = PACKAGE_PLATFORMS.filter((p) => !manifest.platforms?.[p]?.signature);
  if (missingPackages.length) {
    const note = `Updater manifest has no signed entry for ${missingPackages.join(', ')}: installations from those packages will be asked to download the new version manually.`;
    console.warn(process.env.GITHUB_ACTIONS ? `::warning::${note}` : `warning: ${note}`);
  }
  if (problems.length) {
    console.error('Updater manifest is not publishable:\n - ' + problems.join('\n - '));
    process.exit(1);
  }
  const verified = configPath ? ', signatures verified against the app key' : '';
  console.log(`Updater manifest OK: ${manifest.version}, platforms: ${Object.keys(manifest.platforms).join(', ')}${verified}`);
}

const args = process.argv.slice(2);
if (args[0] === '--newer') {
  const candidate = load(args[1]);
  if (!existsSync(args[2])) process.exit(0);
  const current = load(args[2]);
  process.exit(compareVersions(candidate.version, current.version) > 0 ? 0 : 1);
} else if (args.length) {
  const configIndex = args.indexOf('--config');
  const configPath = configIndex >= 0 ? args[configIndex + 1] : null;
  const positional = args.filter((_, i) => configIndex < 0 || (i !== configIndex && i !== configIndex + 1));
  checkManifest(positional[0], positional[1], configPath);
}
