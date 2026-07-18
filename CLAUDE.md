# Nivara — Developer Guide (CLAUDE.md)

Nivara is a **user-level Windows parental activity monitor** built with Tauri 2
(Rust backend) + React/Vite (dashboard). It tracks foreground app usage, idle
time, periodic screenshots, and enforces app/website rules. All data is local
(SQLite + JPEG files under `%LOCALAPPDATA%\Nivara`).

> **Design constraint:** Nivara stays **user-level**. No Windows Service, no
> required elevation for monitoring, no anti-tamper. It is a *deterrent*, not
> hardened protection. See **"What this does and doesn't protect against"** below.

## Architecture

Two binaries in one Cargo package (`src-tauri`):

- **`nivara`** (`src/main.rs` → `lib.rs::run`) — the Tauri dashboard UI + IPC
  commands. Also handles headless CLI modes (see below).
- **`nivara-monitor`** (`src/bin/nivara-monitor.rs` → `monitor::run_loop`) — the
  windowless background agent: samples the foreground window every 5s, logs
  sessions, captures screenshots, evaluates rules, and enforces block rules.

The two communicate only through the SQLite DB (`nivara.db`) and settings keys
(e.g. `monitor_should_run`, `monitor_heartbeat`).

### Rust modules (`src-tauri/src`)
| file | purpose |
|------|---------|
| `db.rs` | schema, settings, data paths (`%LOCALAPPDATA%\Nivara`) |
| `winmon.rs` | Win32 interop: foreground window, idle, screenshot, **process enumeration + termination** |
| `monitor.rs` | the sampling loop; session logging; screenshots; rule eval + **enforcement** |
| `rules.rs` | rule schema, matching, **schedule parsing**, monitor-vs-enforce logic |
| `dns.rs` | **local DNS filtering proxy** (127.0.0.1:53) for website blocking |
| `auth.rs` | **password gate** (PBKDF2 salted hashes + one-time recovery code) |
| `autostart.rs` | HKCU Run-key autostart (points at the installed monitor exe) |
| `commands.rs` | Tauri IPC commands backing the dashboard |
| `lib.rs` | app bootstrap, command registration, headless CLI (`--check-password`, `--uninstall-gate`) |

### Frontend (`src`)
`App.tsx` (single-file views: Dashboard, Timeline, Screenshots, Categories,
Rules, Settings) + `api.ts` (typed `invoke` wrappers). Styling in `App.css`.

## Building & running

### Dev
```powershell
npm install
npm run tauri dev            # runs dashboard; monitor is started from Settings
.\copy-exes.ps1              # build debug exes and copy into dist\ for manual testing
```

### Release installer (NSIS + MSI, bundles WebView2 + monitor sidecar)
```powershell
.\build-installer.ps1
```
This: (1) builds `nivara-monitor` release, (2) stages it as the Tauri
`externalBin` sidecar `src-tauri/binaries/nivara-monitor-<triple>.exe`, (3) runs
`npm run tauri build`. Output installers land in
`src-tauri/target/release/bundle/{nsis,msi}/`.

Packaging notes:
- `tauri.conf.json` → `bundle.targets: ["nsis","msi"]`.
- **WebView2** is embedded via `webviewInstallMode: offlineInstaller` (works
  offline; larger installer).
- The **monitor** ships as an `externalBin` sidecar so it installs *next to*
  `nivara.exe`. Autostart writes an HKCU `Run` value pointing at that installed
  `nivara-monitor.exe` (path resolved relative to `nivara.exe`, i.e. the real
  install dir — not a hardcoded path).
- **Uninstall gate:** `installer-hooks.nsh` (`NSIS_HOOK_PREUNINSTALL`) runs
  `nivara.exe --uninstall-gate`, which prompts for the parent password and
  returns exit 0 (allow) / 1 (deny). NSIS aborts the uninstall on non-zero.

> Because the monitor is a sidecar, `cargo build`/`cargo test` also require the
> sidecar file to exist (Tauri validates `externalBin` at build time). The build
> scripts stage it; if you build cargo directly, copy any `nivara-monitor.exe`
> to `src-tauri/binaries/nivara-monitor-x86_64-pc-windows-msvc.exe` first.

## Features

### 1. Password gate (`auth.rs`) — deterrent only
- First run: the dashboard forces setting a parent password; a **one-time
  recovery code** (`NIVARA-XXXX-XXXX-XXXX`) is shown exactly once. Only PBKDF2
  salted hashes are stored (`auth` table), never plaintext.
- Required to: **stop the monitor** (`stop_monitor` verifies the password
  server-side), **change settings** (the Settings view is locked behind an
  unlock prompt), and **proceed past uninstall** (NSIS pre-uninstall hook).
- Recovery: "Forgot password?" resets the password using the recovery code.
- **Not** process protection: this does not stop Task Manager kills, does not
  convert to a service, and adds no anti-kill logic. A user with account access
  can bypass it.

### 2. Rule enforcement (`rules.rs`, `monitor.rs`, `winmon.rs`, `dns.rs`)
Each rule has `enforcementMode`:
- **`monitor`** — matches are logged as `"would be blocked"` events. Nothing
  happens (unchanged legacy behavior).
- **`enforce`** — the block is actually applied, but only when the global
  **"Enforce block rules"** switch (`enforcement_enabled`) is on **and** the
  rule's schedule (`HH:MM-HH:MM`, optional, overnight-aware) is active.

Enforcement mechanisms:
- **App rules** (`targetType=application`): the monitor enumerates running
  processes every 5s and calls `TerminateProcess` on any whose image name
  contains the rule `target` (e.g. `discord.exe`). Terminations are logged to
  `rule_events`.
- **Website rules** (`targetType=website`): implemented via a **local DNS
  proxy** (`dns.rs`) bound to `127.0.0.1:53`. Blocked domains resolve to
  `0.0.0.0`; everything else is forwarded to the upstream resolver
  (`dns_upstream`, default `1.1.1.1`). The blocklist is rebuilt from the live
  `rules` table every poll (~5s) — `monitor::website_blocklist`. The exact
  domain and its subdomains (e.g. `www.`) are matched. **Window-title matching
  is NOT used for enforcement** — it cannot actually block anything.

  > **Website blocking is driven by `dns_filter_enabled` only** — it is
  > independent of the app-termination switch `enforcement_enabled`. (Earlier
  > the DNS blocklist was gated behind `enforcement_enabled`, so enabling
  > "Website DNS filtering" alone blocked nothing; fixed by decoupling.) The
  > DNS proxy thread starts lazily the first poll `dns_filter_enabled=1` is
  > seen, so toggling it while the monitor runs takes effect without a restart.

  For the proxy to affect the system, the active adapters' DNS must point at
  `127.0.0.1`. Settings → **Enable DNS filtering** does this via an **explicit,
  user-initiated** elevated (UAC) `Set-DnsClientServerAddress` call, and
  **Disable** resets it to DHCP. This is the *only* elevation in Nivara, it is
  visible and reversible, and it is not silent/background.

  **DNS is set to `127.0.0.1` ONLY (no secondary) — and why:**
  - A secondary resolver would defeat blocking two ways: (1) clients can just
    read the real IP of a blocked site from the secondary; (2) **browsers
    auto-upgrade to encrypted DoH** when they detect a known public resolver
    (e.g. `1.1.1.1`, `8.8.8.8`) in the system config, bypassing the proxy
    entirely. So the adapter gets `127.0.0.1` as its *sole* resolver. (An
    earlier build added a `1.1.1.1` secondary "for fail-open" — that was the bug
    that let facebook load despite the block; removed.)
  - The proxy itself is the resolver for everything: it forwards non-blocked
    queries with retries across multiple upstreams (`dns::forward`: configured
    resolver → `1.1.1.1` → `8.8.8.8`) on per-query worker threads, so browsing
    stays fast and reliable while the proxy runs.
  - Not breaking DNS: `dns_filter_apply` **waits for the proxy to actually
    answer on 127.0.0.1:53** (`commands::wait_for_proxy`) before repointing, and
    aborts (DNS unchanged) if it never comes up (e.g. port 53 in use). The proxy
    is kept alive across reboots by monitor autostart. **Trade-off:** if the
    monitor is force-killed while filtering is on, DNS is interrupted until it
    restarts or the user clicks Disable DNS filtering — the deterrent cost of
    airtight blocking.
  - **Explicit browser DoH still bypasses this.** With `127.0.0.1` as the sole
    resolver, Chrome/Edge "automatic" secure-DNS will NOT upgrade, so the common
    case is blocked without touching the browser. But if the user has manually
    set Secure DNS to a custom DoH provider, it bypasses — turn it off. Inherent
    user-level limitation.

### 3. Screenshot viewer
- Click a screenshot to open the lightbox; **ESC closes it** (in addition to
  click-outside).

### 4. Filtering
- **Screenshots** and **Activity Timeline** both have **From/To
  `datetime-local` range filters** (Timeline keeps ‹Prev/Next day›/Today
  shortcuts). Ranges drive the SQLite `captured_at`/`started_at` queries.

### Headless CLI (`lib.rs`)
- `nivara.exe --check-password <pw>` → exit 0 if correct (or no password set),
  1 otherwise. Used by the uninstaller.
- `nivara.exe --uninstall-gate` → prompts for the password (native input box via
  PowerShell) and returns 0/1. Used by the NSIS pre-uninstall hook.

## Verification (real evidence, not code inspection)
Integration tests (`src-tauri/tests/verify.rs`) run the **real binaries** against
a throwaway `%LOCALAPPDATA%`:
```powershell
cd src-tauri
cargo test --test verify -- --test-threads=1 --nocapture
```
- `password_gate_cli`: setup → recovery code issued → verify pass/fail → recover
  → `nivara.exe --check-password` returns **0 (correct) / 1 (wrong)**.
- `enforcement_app_and_dns`: launches the monitor, spawns `notepad.exe`, observes
  it **terminated** (with a logged `rule_events` row), confirms
  `nslookup example.com 127.0.0.1` → **0.0.0.0** (blocked) while
  `wikipedia.org` still resolves to real addresses (forwarded).

Last verified run: notepad terminated within ~5s, `example.com → 0.0.0.0`,
`wikipedia.org → 103.102.166.224`, 1 termination event logged.

## What this does and doesn't protect against

**What Nivara's enforcement DOES:**
- Terminate configured apps shortly after they launch (on-schedule).
- Sinkhole configured domains via a local DNS proxy once system DNS points at it.
- Gate stopping the monitor, changing settings, and uninstalling behind a
  password.

**What it does NOT protect against (all user-level, all bypassable):**
- **Task Manager / `taskkill`** — killing `nivara-monitor.exe` stops all
  monitoring and enforcement. There is no watchdog, no restart-on-kill, no
  service.
- **DNS override** — a user can set their adapter/app DNS back to another
  resolver, or use **DoH/DoT** (browsers with built-in DNS-over-HTTPS bypass the
  system resolver entirely), or a **VPN/proxy**, defeating website blocking.
- **Private/incognito browsing** — does not defeat DNS blocking by itself, but
  Nivara does no per-tab/title enforcement.
- **A different / new Windows user account** — Nivara's data, autostart, and
  password live in the current user's profile (HKCU + `%LOCALAPPDATA%`); another
  account is unaffected.
- **App-name evasion** — renaming an executable avoids the process-name match.
- **Manual file deletion** — deleting the install directory or the DB removes
  everything; the uninstall password gate only guards the NSIS uninstaller.

**Explicitly out of scope (would be a separate, future project):**
Converting to a Windows Service, requiring elevation for monitoring,
tamper-resistance / anti-kill watchdogs, protected processes, or DoH blocking.
Nivara does **not** silently add any of these.
