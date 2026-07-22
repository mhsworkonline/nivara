# Nivara Android — Developer Guide (CLAUDE.md)

> **Status: pre-code.** This is the seed spec for a *new, separate project*.
> Copy this file into the new repo root as `CLAUDE.md` and build from it.
> Nothing here is implemented yet.

Nivara Android is a **local-only app control and network filter** for Android.
It does two things: control which apps can open (lock behind biometric/PIN, or
deny outright on a schedule), and filter domains/IPs at the network level.
Nothing else.

It is a sibling of Nivara desktop (Windows, Tauri + Rust), not a port. It shares
the *philosophy* and the *rule model*; it shares **no code**.

---

## Non-negotiable principles

Carried over from Nivara desktop, and they hold here:

- **Local only.** No backend, no account, no cloud sync, no telemetry. All data
  lives in app-private storage on the device. This is a deliberate reaction to
  account-based tools (Family Link) tangling with the parent's identity.
- **Visible, non-covert deterrent.** The app appears in the launcher and in
  Settings → Apps. No process hiding, no disguising as a system app, **no fake
  force-close / fake crash covers** (a common AppLock-genre feature — do not
  copy it). Deception is out of scope; that is stalkerware territory.
- **No ads, no vault, no themes, no "boosters".** The absence of bloat is the
  product. Every feature added must justify itself against "simple and elegant".
- **Honest about limits.** Ship a deterrent and say so. Do not silently add
  tamper-resistance that changes the threat model without a decision.

---

## Scope

**In scope (v1):**
1. Per-app rules — `ASK` (unlock with biometric/PIN) or `DENY` (blocked, no
   unlock offered), optionally schedule-bound.
2. Domain and IP filtering via a local VPN tunnel.
3. Parent PIN + biometric gate on the app, on settings, and on disabling
   protection. One-time recovery code.

**Explicitly out of scope:**
- Usage statistics, charts, timelines, reports.
- Screenshots (desktop Nivara has them; Android does not get them).
- Photo/video vault, themes, wallpapers.
- Remote parent dashboard / companion device. That requires a backend and a
  whole new threat model.
- iOS. See "iOS reality" below — it is a separate project, not a port.

---

## Distribution: standalone APK

v1 ships as a **sideloaded APK** onto the developer's own devices. This removes
every Play Store policy constraint (accessibility-use declaration,
`QUERY_ALL_PACKAGES` exception, VpnService justification, review cycles).

Two rules that must be settled before the first build:

- **Create a real release keystore on day one and never lose it.** Sign *every*
  build with it, debug included. If early builds use the auto-generated debug
  key, a later switch makes Android refuse to install over the old app — forcing
  an uninstall that wipes the DB and PIN.
- **Increment `versionCode` on every build**, and set a human `versionName`.
  Sideload upgrade = install the new APK over the old; app-private data
  survives, exactly as `%LOCALAPPDATA%` survives on desktop Nivara.

Play Store later is a follow-on task (disclosure screens + policy declarations),
not a rewrite — provided the architecture below is respected.

---

## Architecture

Native **Kotlin + Jetpack Compose**. Not Flutter, not React Native, not Tauri
Mobile: essentially all of this app is platform API surface (accessibility, VPN
tunnel, overlays, device policy), so a cross-platform framework would be a
shared UI over a pile of custom native plugins written anyway — and iOS shares
zero of the enforcement layer regardless.

Single Gradle module (`:app`) to start. Split only when it hurts.

| package | purpose |
|---|---|
| `core/` | rule model, schedule evaluation, PIN/crypto, Room DB |
| `detect/` | accessibility service, foreground-app resolution |
| `enforce/` | overlay engine (lock screen, blocked screen) |
| `netfilter/` | VpnService tunnel, DNS interception, IP dropping |
| `ui/` | Compose screens |
| `system/` | boot receiver, foreground service, permission state machine |

**The boundary that matters:** `enforce/` exposes an interface
(`suspend(app)`, `allow(app)`). v1 implements it with overlays. If Device Owner
is ever added, a second implementation uses `setPackagesSuspended` and nothing
above the interface changes. Skipping this abstraction is what forces a v2
rewrite.

Storage: **Room** for rules/events, **DataStore** for settings, PBKDF2 salted
hash for the PIN (port the *logic* of desktop `auth.rs`, not the code).

---

## How enforcement actually works

**App control and app lock are the same engine.** Do not scope them as two
features.

- **Detection:** `AccessibilityService` (event-driven, instant) with
  `UsageStatsManager` as fallback. Polling alone is too throttled by modern
  Android to feel responsive.
- **Enforcement:** a full-screen overlay activity (`SYSTEM_ALERT_WINDOW`).
  Android apps are not killed — they are covered. `killBackgroundProcesses` is
  unreliable and does not stop a foreground app.
- **`ASK` mode** shows the unlock screen; **`DENY` mode** shows the blocked
  screen with no unlock path. Same overlay, one field difference.

**Network filtering** uses a local `VpnService` — a loopback tunnel, no remote
server. Conceptually the role desktop `dns.rs` plays, entirely different
plumbing:
- Domain blocking → intercept DNS queries in the tunnel, answer `0.0.0.0` for
  blocklist hits, forward everything else.
- IP blocking → drop packets by destination IP. This is *better* than the
  desktop build: real IP-level control.
- Partially counter DoH/DoT by blocking known DoH endpoint IPs and port 853.
  Not airtight.

---

## Rule model

Port the shape from desktop `rules.rs` — the abstraction is already right:

```
Rule {
  id, name,
  targetType: APP | DOMAIN | IP,
  target,                       // package name, domain, or IP/CIDR
  mode: ASK | DENY | ALLOW,
  schedule: "HH:MM-HH:MM"?,     // optional, overnight-aware
  days: Set<DayOfWeek>,
  enabled
}
```

Keep the overnight-aware schedule parsing semantics from desktop — that is real
reusable thinking even though the code is rewritten in Kotlin. Rules
export/import as JSON.

---

## Screens — five, no more

1. **Home** — master protection toggle, live status (accessibility ✓, overlay ✓,
   VPN ✓, battery exempt ✓), active rule count. Any missing permission is stated
   in plain language with a one-tap fix. Elegance lives or dies here.
2. **Apps** — installed apps, searchable; each row cycles Allowed / Ask /
   Denied. Tap for schedule.
3. **Web** — domain and IP list, add/remove, optional schedule.
4. **Settings** — change PIN, recovery code, VPN on/off, export/import rules.
5. **Lock screen** — the modal guarding everything.

**The lock screen is the product's face.** Users see it hundreds of times; every
other screen twice. Put the design effort there.

---

## UX patterns worth copying (AppLock genre)

Reference apps: `applock.lockapps.fingerprint.password.locker`,
`com.rvappstudios.applock.protect.lock.app`. They validate the engine (100M+
installs on the same detection + overlay mechanism). Their *interaction design*
is worth taking; their *security model* is not — they protect a phone from a
30-second guest, this protects a device from a motivated daily user.

**Copy:**
- **`BiometricPrompt` fingerprint unlock, as the default.** PIN is fallback and
  recovery. A PIN-only flow becomes unusable within an hour of real use.
- **Relock policy** — immediately / on screen off / after N minutes. Default to
  screen-off plus a short grace window, or the app re-prompts on every task
  switch and you will rage-quit your own build.
- **Pattern unlock** as an option — faster one-handed, harder to shoulder-surf.
- **Intruder selfie** — front-camera snap after N failed attempts. Cheap, and
  real evidence of bypass attempts.
- **`FLAG_SECURE`** so controlled apps do not leak thumbnails in Recents.

**Do not copy:** vault, themes, ad SDKs, cloud/email recovery, fake
force-close covers.

---

## Onboarding is the hard UX problem

A sideloaded app must ask the user to grant several alarming permissions by
hand. Do it as a guided checklist, one at a time, each with one sentence on
*why*:

1. Set parent PIN → show one-time recovery code (mirror desktop `auth.rs` flow —
   shown exactly once, only hashes stored).
2. Usage access.
3. Display over other apps.
4. Accessibility service — Android shows a genuinely alarming warning; pre-empt
   it with an explanation screen first.
5. VPN consent (only when web filtering is enabled).
6. Battery optimization exemption + OEM autostart hint — detect manufacturer and
   show the right instructions for Xiaomi / Oppo / Vivo / Samsung / OnePlus.

**Re-verify all of these on every launch and surface them on Home.** They get
revoked silently, by the OS or by a curious user.

---

## Build phases

Each phase ends in an installable APK tested on a real device. "Real evidence"
on mobile means the APK on the phone — not unit tests. (Same discipline as
desktop Nivara's `tests/verify.rs`.)

| Phase | Content | Effort (sessions) |
|---|---|---|
| 1 | Foundation: DB, rule model, PIN + recovery, permission state machine, Home. No enforcement. Must survive reboot. | 2–3 |
| 2 | **App lock (`ASK`)**: accessibility detection + overlay + biometric. Overlay must be fast and not escapable via Back. | 3–4 |
| 3 | Schedules + `DENY` mode | 1–2 |
| 4 | UI polish | 2–3 |
| 5 | VPN filtering: DNS interception first, IP dropping second | 5–10 |
| 6 | Survival hardening: foreground service, boot restart, OEM quirks | 3–8 |
| 7 | *(optional, deferred)* Device Owner / uninstall resistance | 3–6 |

**Build Phase 2 before Phase 3** — `ASK` mode is useful on day one, has a proven
UX to copy, and exercises the whole detection → overlay → unlock loop.
Schedule-based `DENY` is then a small addition, not new machinery.

**Phases 1–4 are the simple project** (~8–12 sessions, a few weekends) and
deliver a genuinely useful tool. **Decide on Phase 5 only after 1–4 have run on
a real phone for a couple of weeks** — blocking the browser on a schedule is a
two-line rule; blocking domains inside the browser is a week of packet parsing.

---

## Cost reality

**Money: zero.** No backend, no accounts, no recurring anything. Free tooling.
The only cost that ever appears is the one-time $25 Play Console fee, if
publishing later.

**Battery:**
- Accessibility detection — event-driven, well under 1%/day.
- Overlay — exists only while blocking. Free.
- Foreground service notification — free.
- **VpnService is the entire battery story.** A well-written tunnel that
  inspects DNS and passes everything else straight through costs ~3–8%/day. A
  naive one that busy-loops on the file descriptor or allocates per packet costs
  20%+ and warms the phone. The difference is implementation quality — be
  careful here rather than fast.

**Other VPN costs:** a permanent key icon in the status bar, and **no other VPN
can run** — Android permits exactly one. Mutually exclusive with a work VPN on
the same device.

**Where the effort actually goes:** features are ~40%. Surviving Android process
management and OEM battery killers is ~30%. The permission/onboarding state
machine is ~20%. Scope the calendar around the latter two.

---

## Testing

- Two devices: a daily driver as reference, an older one as the abusable target.
- `adb` wireless debugging set up early.
- **Do not develop the VPN filter against the phone you depend on.** A
  half-working tunnel means no internet mid-day.
- The VPN tunnel and Android Studio's debugger interact badly. Test net
  filtering with the debugger detached and log to a file or Room table rather
  than relying on logcat.

---

## What this does and does not protect against

**Does:**
- Gate app launches behind biometric/PIN, or deny them on schedule.
- Sinkhole configured domains and drop configured IPs while the VPN runs.
- Gate settings changes and protection-off behind the parent PIN.

**Does not (all bypassable, all user-level):**
- **Disabling the accessibility service** in Settings — the primary bypass.
- **Force-stop** from Settings → Apps kills the overlay service on most OEM
  skins until the foreground service restarts it.
- **Uninstall** — nothing prevents it without Device Owner (Phase 7).
- **Another VPN app** — Android's single-VPN slot means any other VPN disables
  filtering entirely.
- **Explicit DoH in a browser** — bypasses DNS filtering; same limitation as
  desktop Nivara.
- **A second user profile / work profile** — unaffected.
- **Safe mode** — accessibility services do not run.

**Out of scope entirely:** covert operation, process hiding, disguising the app,
detection evasion, silent installation.

---

## iOS reality (for later, if ever)

There is no `VpnService`/Accessibility equivalent. The only sanctioned path is
the **Screen Time API** (`FamilyControls` + `ManagedSettings` +
`DeviceActivity`), which requires a **special Apple entitlement** — application
required, weeks of lead time, approval not guaranteed. The app never learns
which app is running; it hands Apple opaque tokens and Apple enforces. Almost
none of the Android UX is reproducible.

**Therefore: design Android for Android.** Do not compromise the Android
architecture for a hypothetical port. If iOS matters, start the entitlement
application early — the paperwork is the long pole, not the code.

---

## Positioning

**AppLock's polish, plus schedules and network filtering — no ads, no account,
no vault, no cloud.**

That one line is the scope guard. When a feature is proposed, check it against
that sentence first.
