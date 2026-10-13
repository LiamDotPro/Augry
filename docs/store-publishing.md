# Store publishing

How Augry gets into the Microsoft Store, and why the Mac App Store is
not a target. Checked against the Microsoft and Tauri docs on 24 September
2026; links at the end.

## Microsoft Store

### Two routes

| | MSIX (recommended) | EXE/MSI |
|---|---|---|
| Package | `scripts/pack-msix.ps1` | `npm run tauri build -- --config src-tauri/tauri.microsoftstore.conf.json` |
| Signing | Store re-signs it, no certificate needed | Must be Authenticode signed with a CA-issued certificate before submission; the Store does not sign it |
| Hosting | Uploaded to Partner Center, served from the Store CDN | Hosted on our own versioned HTTPS URL (a GitHub Release asset works) that must never change |
| Updates | Store delivers them | We submit a new URL per version, and in-app updates need the updater plugin |
| WebView2 | Must already be on the machine (it is on Windows 11 and on any Windows 10 with Edge) | Offline installer with the runtime embedded, about 150 MB larger |
| Install UX | Standard Store install, no UAC | Silent install with `/S` (NSIS) or `/quiet` (MSI) |

MSIX is the route to take. A code signing certificate is the only real cost
of the EXE route: a few hundred dollars a year from DigiCert, Sectigo or
GlobalSign, or a monthly fee for Azure Artifact Signing. The EXE overlay
config stays in the repo for the day we want signed installers outside the
Store as well.

MSIX packages run with `runFullTrust`, the same as Windows Terminal, so
spawning `pwsh.exe` through ConPTY and running `git.exe` works unchanged. The
manifest also disables filesystem and registry write virtualization so shells
started from the app write to the real AppData and registry rather than the
package container. That needs the restricted `unvirtualizedResources`
capability, which Partner Center asks you to justify in the submission. The
justification: "Terminal emulator. Shells and developer tools launched by the
user must write to their real AppData and registry, as in Windows Terminal,
which declares the same capability."

### One-time setup

1. Register at <https://partner.microsoft.com/dashboard/registration>. The
   individual developer account is a one-time fee (USD 19 at the time of
   writing; company accounts are USD 99 and need business verification).
2. Apps and games > New product > **MSIX or PWA app**. Reserve the name
   "Augry".
3. Open Product management > **Product identity** and copy
   `Package/Identity/Name` (looks like `12345LiamRead.Augry`) and
   `Package/Identity/Publisher` (looks like `CN=<GUID>`) into
   `src-tauri/msix/identity.json`. Those two values must match the manifest
   exactly or the upload is rejected.
4. Bump the app to a `1.x` version. The Store rejects packages whose major
   version is 0, and reserves the fourth part, so `1.0.0` in `tauri.conf.json`
   becomes `1.0.0.0` in the package.

### Each release

```
npm run tauri build
.\scripts\pack-msix.ps1
```

Upload `src-tauri\target\msix\Augry_<version>_x64.msix` on the
submission's Packages page. The release workflow builds the same file on every
tag and keeps it as a workflow artifact named `msix-unsigned`, so it can be
downloaded from the Actions run instead of built locally.

Before the first upload, run the Windows App Certification Kit against the
package (Visual Studio Installer > Individual components > Windows App
Certification Kit, then
`appcert.exe test -appxpackagepath <msix> -reportoutputpath report.xml`).
It catches manifest and asset problems before certification does.

### Local install test

```
.\scripts\pack-msix.ps1 -SelfSign
```

This signs with a self-signed certificate and prints the two commands to
trust it (elevated shell, once) and install it. Alternatively turn on
Developer Mode in Settings > System > For developers and register the loose
layout without signing:

```
Add-AppxPackage -Register src-tauri\target\msix\layout\AppxManifest.xml
```

Remove it afterwards with `Get-AppxPackage *Augry* | Remove-AppxPackage`.
The Store build is uploaded unsigned; never upload the self-signed one.

Things to check in the installed package: a pane opens a `pwsh` shell, git
labels appear for a repo, and `echo $env:APPDATA` from a pane prints the real
Roaming folder, not a path under `AppData\Local\Packages`.

### Listing checklist

- Description, short description, and at least one 1366x768 or larger
  screenshot. Four to six screenshots of the grid, worktree labels, agent
  detection and Notes are enough.
- Store logo: `src-tauri/icons/Square310x310Logo.png` resized to 300x300.
- Privacy policy URL. Full-trust apps are asked for one; a short page in the
  GitHub repo stating that the app stores nothing and sends nothing is enough.
- Age rating through the IARC questionnaire (a developer tool with no user
  generated content rates as "Everyone").
- Category: Developer tools.
- Pricing: free, all markets.
- Certification takes one to three business days. After approval the package
  is signed with Microsoft's certificate and served from the Store CDN.

### What the manifest does not cover yet

- Only x64. Add an Arm64 build (`--target aarch64-pc-windows-msvc`) and a
  second package once there is a machine to test it on.
- Only unscaled icons. Adding `scale-125` to `scale-400` variants of
  `Square44x44Logo` and `Square150x150Logo` (plus a `resources.pri` via
  `makepri`) gives crisper taskbar and Start icons; the certification kit only
  warns about this.
- No large or wide tiles. `Square310x310Logo` is only allowed together with a
  `Wide310x150Logo`, which the icon set does not have, so the manifest declares
  neither and Start falls back to the 150x150 tile.

## Mac App Store

Not viable for this app as it stands, for two reasons.

1. **No macOS build yet.** The core has a `cfg(not(windows))` shell path and a
   bash/zsh integration script, but it has never been built or run on macOS:
   no `.icns`, no `bundle.category`, no entitlements, no notarization, and
   the pwsh detection, `CREATE_NO_WINDOW` flags and ConPTY assumptions are all
   Windows-only.
2. **App Sandbox is mandatory in the Mac App Store** and it applies to child
   processes. A sandboxed app may spawn a shell, but that shell inherits the
   sandbox: it can only read and write folders the user picked through the
   open panel, cannot exec another sandboxed binary, and cannot see the
   user's home, `.gitconfig`, SSH keys or Homebrew. That is why iTerm2, Warp,
   Alacritty, kitty and Ghostty are all distributed outside the App Store.
   Augry's whole job is running shells and git in arbitrary folders,
   so a sandboxed build would be a different, mostly broken product.

If macOS becomes a target, the route is: port the shell layer, then ship a
notarized DMG through GitHub Releases (Developer ID certificate, USD 99 a
year, `tauri build --bundles dmg` plus the notarization built into
`tauri-action`), and publish a Homebrew cask. No store review is involved and
the app runs unsandboxed, like every other terminal on macOS.

## Sources

- Tauri, Microsoft Store: <https://tauri.app/distribute/microsoft-store/>
- Tauri, App Store: <https://tauri.app/distribute/app-store/>
- Microsoft, MSIX package requirements (signing, version rules):
  <https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements>
- Microsoft, EXE/MSI package requirements:
  <https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements>
- Microsoft, winapp CLI with Tauri (the same MSIX layout built with their
  preview CLI; needs Windows 11):
  <https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri>
- Windows Terminal's manifest, the precedent for `runFullTrust` plus
  `unvirtualizedResources`:
  <https://github.com/microsoft/terminal/blob/main/src/cascadia/CascadiaPackage/Package.appxmanifest>
- Apple, App Sandbox:
  <https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox>
