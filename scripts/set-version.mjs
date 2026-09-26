#!/usr/bin/env node
// Sets the application version everywhere it is recorded.
//   npm run version:set -- 1.2.3
// Updates Cargo.toml (workspace version), package.json/package-lock.json and
// Cargo.lock, and reminds you to add a CHANGELOG.md section.
import { execSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2]?.replace(/^v/, '');
if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error('usage: npm run version:set -- <semver>   e.g. 1.2.3 or 1.3.0-beta.1');
  process.exit(2);
}

const cargo = readFileSync('Cargo.toml', 'utf8');
const updated = cargo.replace(
  /(\[workspace\.package\][^[]*?\nversion = ")[^"]*(")/,
  `$1${version}$2`,
);
if (updated === cargo && !cargo.includes(`version = "${version}"`)) {
  console.error('could not find [workspace.package] version in Cargo.toml');
  process.exit(1);
}
writeFileSync('Cargo.toml', updated);

execSync(`npm version ${version} --no-git-tag-version --allow-same-version`, { stdio: 'inherit' });
execSync('cargo update --workspace --offline', { stdio: 'inherit' });

const changelog = readFileSync('CHANGELOG.md', 'utf8');
if (!new RegExp(`^## \\[?${version.replace(/\./g, '\\.')}\\]?`, 'm').test(changelog)) {
  console.warn(`\nReminder: add a "## [${version}] - YYYY-MM-DD" section to CHANGELOG.md before tagging.`);
}
console.log(`\nVersion set to ${version}. Commit, then: git tag v${version} && git push origin v${version}`);
