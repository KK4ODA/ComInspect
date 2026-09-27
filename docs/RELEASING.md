# Releasing ComInspect

This page is for maintainers. It covers the one-time setup of the update signing key, and how
stable and beta versions are published.

## How a release works

Pushing a version tag starts `.github/workflows/release.yml`:

```
git tag v1.2.3 && git push origin v1.2.3
  └─► Release workflow
       1. prepare  checks that the tag matches Cargo.toml and package.json,
                   that the update signing key is configured, and that
                   CHANGELOG.md has a section for the version; creates a
                   *draft* GitHub release with those notes
       2. build    one job per platform (Windows x64, macOS Apple silicon,
                   macOS Intel, Linux x64; Linux ARM64 optional):
                   unit tests, then tauri-action builds the installers,
                   signs the update files and uploads everything, including
                   latest.json, to the draft
       3. publish  verifies latest.json (every platform present and signed),
                   publishes the release and refreshes the beta feed
```

- **Why a draft first.** Installed apps read the stable feed from
  `https://github.com/KK4ODA/ComInspect/releases/latest/download/latest.json`. That URL only
  follows *published* releases, so users never see a half-uploaded one.
- **Release notes.** The `CHANGELOG.md` section for the version becomes the release notes shown in
  the app's update dialog.

## One-time setup

### 1. Create the update signing key (required)

Every update is signed with a private key and checked by the app against the matching public key.
Nothing can be published until the key exists.

> **Status:** this repository's public key, ID `ACBF14CD8E8324F8`, is in
> `src-tauri/tauri.conf.json`. Its private key and password belong in the two repository secrets
> below. Copies of ComInspect installed with that key accept only updates signed with it, so
> don't replace it unless it is lost or leaked. Replacing it means every user has to install the
> next version by hand.

To create a key, run this on your own computer. It needs Node.js but not a copy of the repository.

macOS or Linux:

```sh
npx @tauri-apps/cli@2 signer generate -w ~/.tauri/cominspect.key
```

Windows (PowerShell):

```powershell
npx.cmd "@tauri-apps/cli@2" signer generate -w "$env:USERPROFILE\.tauri\cominspect.key"
```

On Windows, use `npx.cmd` rather than `npx`. PowerShell's default script policy blocks `npx` with
"running scripts is disabled on this system". Running the command in Command Prompt (`cmd`)
avoids that as well; there the path is `"%USERPROFILE%\.tauri\cominspect.key"`.

Choose a strong password when asked; nothing appears while you type it. This creates:

| File | Contents | Share it? |
|---|---|---|
| `cominspect.key` | The private key, about 350 characters | Never |
| `cominspect.key.pub` | The public key, about 150 characters | Safe to share |

Both files are one long line starting with `dW50cnVzdGVk`. Tell them apart by the `.pub` ending.

1. **Back up the private key and its password** before anything else, in a password manager or
   another safe place:
   - **If you lose the key or password,** installed copies can never accept another automatic
     update. Every user would have to download a new version by hand.
   - **If the key leaks,** someone with it *and* the ability to publish releases on this
     repository could ship a malicious update. Treat the key like a password.
2. In the GitHub repository go to **Settings → Secrets and variables → Actions → New repository
   secret** and add:

   | Secret | Value |
   |---|---|
   | `TAURI_SIGNING_PRIVATE_KEY` | The entire contents of `cominspect.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The password you chose |

   To copy the key, open it in a plain text editor (`notepad "$env:USERPROFILE\.tauri\cominspect.key"`
   on Windows) or run `pbcopy < ~/.tauri/cominspect.key` on macOS.
3. Put the **entire contents** of `cominspect.key.pub` into `src-tauri/tauri.conf.json` as the
   value of `plugins.updater.pubkey`, and commit the change. Builds without a key show
   "automatic updates are disabled".
4. Never commit the private key. `.gitignore` already excludes `*.key`.

**Checking the password (optional).** Signing any small file confirms that the key and password
belong together:

```sh
npx @tauri-apps/cli@2 signer sign -f ~/.tauri/cominspect.key ~/.tauri/cominspect.key.pub
```

It asks for the password. *"Your file was signed successfully"* means they match; delete the
`.sig` file it creates.

**Safety net.** Before publishing, the release workflow verifies every signature in `latest.json`
against the public key in `tauri.conf.json`. A release signed with a different key, or without
the version the app requires, stays a draft instead of reaching users.

### 2. Workflow permissions

The release workflow declares `permissions: contents: write`, which is enough for a personal
repository. If a release job fails with `403` when creating the release, check **Settings →
Actions → General → Workflow permissions**.

### 3. Windows code signing (optional)

Unsigned Windows installers work, but SmartScreen warns users on first launch. Code signing
removes that warning. Update security doesn't depend on it, because updates are verified with the
key from step 1.

**Option A: a certificate exported as PFX.** The workflow can use it directly:

| Secret | Value |
|---|---|
| `WINDOWS_CERTIFICATE` | The `.pfx` file, base64-encoded (`base64 -w0 cert.pfx`) |
| `WINDOWS_CERTIFICATE_PASSWORD` | The PFX password |

**Option B: a hardware token or cloud signing service.** Code-signing certificates issued since
mid-2023 must be stored in hardware, so most new certificates can't be exported as a PFX. Use a
cloud signing service instead, such as:

- Azure Trusted Signing
- SignPath, which is free for open-source projects
- a certificate authority's cloud HSM

Configure the service's command line as `bundle.windows.signCommand` in a config file passed to
the build, the same way the PFX step does.

### 4. macOS signing and notarization (optional)

Without an Apple Developer ID the workflow signs the app *ad hoc*. It runs, but on first launch
users must allow it in **System Settings → Privacy & Security**. Developer ID signing and
notarization remove that step. They need a paid Apple Developer Program membership and these
secrets:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | The *Developer ID Application* certificate exported as `.p12`, base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` password |
| `APPLE_SIGNING_IDENTITY` | For example `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | Your Apple ID email |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID |
| `APPLE_TEAM_ID` | Your team ID |

## Publishing a stable version

1. **Write the release notes.** Add a section to `CHANGELOG.md`:

   ```markdown
   ## [1.2.3] - 2026-10-01

   - What changed, in words users understand.
   ```

2. **Set the version** everywhere it is recorded: `Cargo.toml`, `package.json` and both lockfiles.

   ```sh
   npm run version:set -- 1.2.3
   ```

3. **Commit and push to the default branch, and wait for CI to pass.**
4. **Tag and push the tag:**

   ```sh
   git tag v1.2.3
   git push origin v1.2.3
   ```

5. **Watch the Release workflow** under the repository's **Actions** tab. It takes about 15–25
   minutes. When it finishes, the release is public and installed apps find it at their next check.
6. **Check it.** Start an older installed version and choose **Menu → About & updates → Check for
   updates**.

## Publishing a beta version

- **Tag.** Tag a pre-release version, for example `v1.3.0-beta.1`. Set it the same way:
  `npm run version:set -- 1.3.0-beta.1`, plus a `## [1.3.0-beta.1]` CHANGELOG section.
- **Where it goes.**
  - The release is published as a GitHub *pre-release*.
  - `releases/latest` ignores pre-releases, so stable users are never offered it.
  - The workflow copies the release's `latest.json` to the `updater-feeds` release as `beta.json`,
    which is the feed that beta-channel users read.
- **Stable releases** also update `beta.json` when they are newer. Beta users therefore move on to
  the next stable version automatically.
- **Promoting a beta** means tagging the final version, for example `v1.3.0`. It is a normal stable
  release, built again from that tag.

The `updater-feeds` release is created automatically the first time. It is marked as a
pre-release and holds only `beta.json`. Don't delete it.

## When something goes wrong

- **A build fails.** The release stays a draft and users see nothing.
  - To fix the workflow or code: delete the draft release and the tag
    (`git push --delete origin v1.2.3 && git tag -d v1.2.3`), commit the fix, then tag again.
  - To retry without changing anything, use **Re-run failed jobs**, or run the Release workflow by
    hand (**Actions → Release → Run workflow**) with the existing tag.
- **"Updater manifest is not publishable."** A platform's update file or signature is missing, so
  the release was not published. Check that platform's build log.
  - A missing `.msi`, `.deb` or `.rpm` entry is only a warning. Users who installed from those
    packages are asked to download the new version by hand.
- **A published release is broken.**
  - Publish a fixed version as soon as possible. Updates only move forward, so users on the broken
    version will receive the fix.
  - Meanwhile you can mark the previous release as latest (**Edit release → Set as the latest
    release**). New update checks then stop offering the broken one.
  - Users already on it can use **About & updates → Advanced → Reinstall previous version**.
- **Keep old releases and their `latest.json` files.** "Reinstall previous version" downloads the
  older version through that release's own `latest.json` and verifies its signature, so deleting
  old releases removes the way back.

## Release checklist

- [ ] CHANGELOG section written, in words users understand
- [ ] `npm run version:set -- X.Y.Z` run and committed
- [ ] CI is green on the default branch
- [ ] Tag `vX.Y.Z` pushed
- [ ] Release workflow finished; the release is published with installers for every platform
- [ ] An older installed copy offers the update and installs it
