#!/usr/bin/env node
// Validates the updater manifest (latest.json) produced by tauri-action.
//
//   node scripts/check-updater-manifest.mjs latest.json 1.2.3
//     Fails unless the manifest announces 1.2.3 and has a signed download for
//     every required platform.
//
//   node scripts/check-updater-manifest.mjs --newer candidate.json current.json
//     Exits 0 when candidate.json announces a newer version than
//     current.json (or current.json does not exist), 1 otherwise.
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

function load(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

const args = process.argv.slice(2);
if (args[0] === '--newer') {
  const candidate = load(args[1]);
  if (!existsSync(args[2])) process.exit(0);
  const current = load(args[2]);
  process.exit(compareVersions(candidate.version, current.version) > 0 ? 0 : 1);
}

const [file, expected] = args;
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
const missingPackages = PACKAGE_PLATFORMS.filter((p) => !manifest.platforms?.[p]?.signature);
if (missingPackages.length) {
  const note = `Updater manifest has no signed entry for ${missingPackages.join(', ')}: installations from those packages will be asked to download the new version manually.`;
  console.warn(process.env.GITHUB_ACTIONS ? `::warning::${note}` : `warning: ${note}`);
}
if (problems.length) {
  console.error('Updater manifest is not publishable:\n - ' + problems.join('\n - '));
  process.exit(1);
}
console.log(`Updater manifest OK: ${manifest.version}, platforms: ${Object.keys(manifest.platforms).join(', ')}`);
