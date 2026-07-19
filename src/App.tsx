import { useEffect, useMemo, useState } from "react";
import {
  api,
  CATEGORY_COLORS,
  fmtDateTime,
  fmtDuration,
  fmtTime,
  fromLocalInput,
  startOfDay,
  startOfWeek,
  toLocalInput,
  type ActivityRow,
  type Category,
  type MonitorStatus,
  type Report,
  type Rule,
  type RuleEvent,
  type ScreenshotRow,
  type SettingRow,
} from "./api";
import "./App.css";

type View = "dashboard" | "timeline" | "screenshots" | "categories" | "rules" | "settings";

const CATEGORIES: Category[] = ["Study", "Social", "Entertainment", "Other"];

export default function App() {
  const [view, setView] = useState<View>("dashboard");
  const [status, setStatus] = useState<MonitorStatus | null>(null);
  const [authSet, setAuthSet] = useState<boolean | null>(null);
  // Verified parent password held in-memory for the session (null = locked).
  const [pw, setPw] = useState<string | null>(null);

  useEffect(() => {
    api.authStatus().then(setAuthSet).catch(() => setAuthSet(false));
  }, []);

  useEffect(() => {
    const load = () => api.monitorStatus().then(setStatus).catch(() => {});
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, []);

  if (authSet === null) {
    return <div className="app-loading">Loading…</div>;
  }
  if (authSet === false) {
    return (
      <FirstRunSetup
        onDone={(password) => {
          setAuthSet(true);
          setPw(password);
        }}
      />
    );
  }
  // Password is required to open the app: stay locked until it's entered.
  if (pw === null) {
    return <UnlockGate onUnlock={setPw} fullScreen />;
  }

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="brand">
          Niv<span>ara</span>
          <small>Parental Activity Monitor</small>
        </div>
        {(
          [
            ["dashboard", "Dashboard"],
            ["timeline", "Activity Timeline"],
            ["screenshots", "Screenshots"],
            ["categories", "Categories"],
            ["rules", "Rules"],
            ["settings", "Settings"],
          ] as [View, string][]
        ).map(([k, label]) => (
          <button
            key={k}
            className={`navbtn ${view === k ? "active" : ""}`}
            onClick={() => setView(k)}
          >
            {label}
          </button>
        ))}
        <button className="navbtn lock" onClick={() => setPw(null)}>
          🔒 Lock
        </button>
        <div className="status-pill">
          <div>
            <span className={`dot ${status?.running ? "on" : "off"}`} />
            Monitor {status?.running ? "running" : "stopped"}
          </div>
          {status && status.heartbeat > 0 && (
            <div style={{ marginTop: 4 }}>
              Last beat {status.seconds_since_heartbeat}s ago
            </div>
          )}
        </div>
      </nav>
      <main className="main">
        {view === "dashboard" && <Dashboard status={status} />}
        {view === "timeline" && <Timeline />}
        {view === "screenshots" && <Screenshots />}
        {view === "categories" && <Categories />}
        {view === "rules" && <Rules />}
        {view === "settings" && (
          <Settings pw={pw} onMonitor={() => api.monitorStatus().then(setStatus)} />
        )}
      </main>
    </div>
  );
}

function FirstRunSetup({ onDone }: { onDone: (pw: string) => void }) {
  const [p1, setP1] = useState("");
  const [p2, setP2] = useState("");
  const [err, setErr] = useState("");
  const [recovery, setRecovery] = useState<string | null>(null);
  const [pending, setPending] = useState("");

  const submit = () => {
    setErr("");
    if (p1.length < 4) return setErr("Password must be at least 4 characters.");
    if (p1 !== p2) return setErr("Passwords do not match.");
    api
      .authSetup(p1)
      .then((code) => {
        setPending(p1);
        setRecovery(code);
      })
      .catch((e) => setErr(String(e)));
  };

  if (recovery) {
    return (
      <div className="gate-screen">
        <div className="gate-card">
          <h1>Save your recovery code</h1>
          <p className="sub">
            This is shown <strong>once</strong>. If you forget the password, this code is the only
            way to reset it. Store it somewhere safe.
          </p>
          <div className="recovery-code">{recovery}</div>
          <button className="btn" onClick={() => onDone(pending)}>
            I've saved it — continue
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="gate-screen">
      <div className="gate-card">
        <div className="brand" style={{ marginBottom: 18 }}>
          Niv<span>ara</span>
          <small>Parental Activity Monitor</small>
        </div>
        <h1>Set a parent password</h1>
        <p className="sub">
          Required to stop the monitor, change settings, and uninstall. This is a deterrent, not
          tamper-proof protection.
        </p>
        <input
          type="password"
          placeholder="Password"
          value={p1}
          onChange={(e) => setP1(e.target.value)}
        />
        <input
          type="password"
          placeholder="Confirm password"
          value={p2}
          onChange={(e) => setP2(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
        {err && <div className="gate-err">{err}</div>}
        <button className="btn" onClick={submit}>
          Set password
        </button>
      </div>
    </div>
  );
}

function UnlockGate({
  onUnlock,
  fullScreen,
}: {
  onUnlock: (pw: string) => void;
  fullScreen?: boolean;
}) {
  const [pw, setPw] = useState("");
  const [err, setErr] = useState("");
  const [recover, setRecover] = useState(false);
  const [code, setCode] = useState("");
  const [np, setNp] = useState("");

  const submit = () => {
    setErr("");
    api
      .authVerify(pw)
      .then((ok) => (ok ? onUnlock(pw) : setErr("Incorrect password.")))
      .catch((e) => setErr(String(e)));
  };

  const doRecover = () => {
    setErr("");
    api
      .authRecover(code.trim(), np)
      .then(() => onUnlock(np))
      .catch((e) => setErr(String(e)));
  };

  return (
    <div className={fullScreen ? "gate-screen" : "gate-inline"}>
      <div className="gate-card">
        {fullScreen && (
          <div className="brand" style={{ marginBottom: 12 }}>
            Niv<span>ara</span>
            <small>Parental Activity Monitor</small>
          </div>
        )}
        <h1>{recover ? "Reset password" : fullScreen ? "🔒 Locked" : "Locked"}</h1>
        <p className="sub">
          {recover
            ? "Enter your recovery code and a new password."
            : fullScreen
            ? "Enter the parent password to open Nivara."
            : "Enter the parent password to access Settings."}
        </p>
        {!recover ? (
          <>
            <input
              type="password"
              placeholder="Password"
              value={pw}
              onChange={(e) => setPw(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && submit()}
              autoFocus
            />
            {err && <div className="gate-err">{err}</div>}
            <button className="btn" onClick={submit}>
              Unlock
            </button>
            <button className="linkbtn" onClick={() => setRecover(true)}>
              Forgot password? Use recovery code
            </button>
          </>
        ) : (
          <>
            <input
              type="text"
              placeholder="Recovery code (NIVARA-XXXX-XXXX-XXXX)"
              value={code}
              onChange={(e) => setCode(e.target.value)}
            />
            <input
              type="password"
              placeholder="New password"
              value={np}
              onChange={(e) => setNp(e.target.value)}
            />
            {err && <div className="gate-err">{err}</div>}
            <button className="btn" onClick={doRecover}>
              Reset & unlock
            </button>
            <button className="linkbtn" onClick={() => setRecover(false)}>
              Back
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function useReport(mode: "day" | "week") {
  const [report, setReport] = useState<Report | null>(null);
  const start = useMemo(
    () => (mode === "day" ? startOfDay(new Date()) : startOfWeek(new Date())),
    [mode]
  );
  useEffect(() => {
    const p = mode === "day" ? api.dailyReport(start) : api.weeklyReport(start);
    p.then(setReport).catch(() => {});
  }, [mode, start]);
  return report;
}

function Dashboard({ status }: { status: MonitorStatus | null }) {
  const [mode, setMode] = useState<"day" | "week">("day");
  const report = useReport(mode);
  const catMax = Math.max(1, ...(report?.by_category.map((c) => c.total_secs) ?? [1]));
  const appMax = Math.max(1, ...(report?.top_apps.map((a) => a.total_secs) ?? [1]));

  return (
    <>
      <h1>Dashboard</h1>
      <p className="sub">
        {mode === "day" ? "Today's" : "This week's"} activity from real collected data.
      </p>

      {status && !status.running && (
        <div className="banner">
          The background monitor is not running. Start it from Settings to begin collecting activity.
        </div>
      )}

      <div className="controls">
        <button className={`btn ${mode === "day" ? "" : "ghost"}`} onClick={() => setMode("day")}>
          Daily
        </button>
        <button className={`btn ${mode === "week" ? "" : "ghost"}`} onClick={() => setMode("week")}>
          Weekly
        </button>
      </div>

      <div className="cards">
        <div className="card">
          <div className="label">Active Time</div>
          <div className="value">{fmtDuration(report?.total_active_secs ?? 0)}</div>
        </div>
        <div className="card">
          <div className="label">Idle Time</div>
          <div className="value">{fmtDuration(report?.idle_secs ?? 0)}</div>
        </div>
        <div className="card">
          <div className="label">Apps Used</div>
          <div className="value">{report?.top_apps.length ?? 0}</div>
        </div>
        <div className="card">
          <div className="label">Top Category</div>
          <div className="value" style={{ fontSize: 20 }}>
            {report?.by_category[0]?.category ?? "—"}
          </div>
        </div>
      </div>

      <div className="panel">
        <h2>Time by Category</h2>
        {report && report.by_category.length > 0 ? (
          report.by_category.map((c) => (
            <div className="bar-row" key={c.category}>
              <div className="bar-label">{c.category}</div>
              <div className="bar-track">
                <div
                  className="bar-fill"
                  style={{
                    width: `${(c.total_secs / catMax) * 100}%`,
                    background: CATEGORY_COLORS[c.category] ?? "#64748b",
                  }}
                />
              </div>
              <div className="bar-val">{fmtDuration(c.total_secs)}</div>
            </div>
          ))
        ) : (
          <div className="empty">No activity recorded yet.</div>
        )}
      </div>

      <div className="panel">
        <h2>Top Applications</h2>
        {report && report.top_apps.length > 0 ? (
          report.top_apps.map((a) => (
            <div className="bar-row" key={a.app_name}>
              <div className="bar-label" title={a.app_name}>
                {a.app_name}
              </div>
              <div className="bar-track">
                <div
                  className="bar-fill"
                  style={{
                    width: `${(a.total_secs / appMax) * 100}%`,
                    background: CATEGORY_COLORS[a.category ?? "Other"] ?? "#64748b",
                  }}
                />
              </div>
              <div className="bar-val">{fmtDuration(a.total_secs)}</div>
            </div>
          ))
        ) : (
          <div className="empty">No activity recorded yet.</div>
        )}
      </div>
    </>
  );
}

function Timeline() {
  const [rows, setRows] = useState<ActivityRow[]>([]);
  const [from, setFrom] = useState(() => toLocalInput(startOfDay(new Date()) * 1000));
  const [to, setTo] = useState(() => toLocalInput((startOfDay(new Date()) + 86400) * 1000));

  useEffect(() => {
    api
      .activityTimeline(fromLocalInput(from), fromLocalInput(to), 2000)
      .then(setRows)
      .catch(() => setRows([]));
  }, [from, to]);

  const shiftDay = (deltaDays: number) => {
    const f = fromLocalInput(from) + deltaDays * 86400;
    const t = fromLocalInput(to) + deltaDays * 86400;
    setFrom(toLocalInput(f * 1000));
    setTo(toLocalInput(t * 1000));
  };

  return (
    <>
      <h1>Activity Timeline</h1>
      <p className="sub">Chronological record of foreground applications.</p>
      <div className="controls">
        <button className="btn ghost" onClick={() => shiftDay(-1)}>
          ‹ Previous day
        </button>
        <label className="range-label">
          From
          <input type="datetime-local" value={from} onChange={(e) => setFrom(e.target.value)} />
        </label>
        <label className="range-label">
          To
          <input type="datetime-local" value={to} onChange={(e) => setTo(e.target.value)} />
        </label>
        <button className="btn ghost" onClick={() => shiftDay(1)}>
          Next day ›
        </button>
        <button
          className="btn ghost"
          onClick={() => {
            setFrom(toLocalInput(startOfDay(new Date()) * 1000));
            setTo(toLocalInput((startOfDay(new Date()) + 86400) * 1000));
          }}
        >
          Today
        </button>
      </div>
      <div className="panel">
        {rows.length === 0 ? (
          <div className="empty">No activity for this day.</div>
        ) : (
          rows.map((r) => (
            <div className="timeline-item" key={r.id}>
              <div className="timeline-time">
                {fmtTime(r.started_at)} – {fmtTime(r.ended_at)}
              </div>
              <div className="timeline-body">
                <div className="app">
                  {r.is_idle ? "💤 Idle" : r.app_name}
                  {r.category && !r.is_idle && (
                    <span
                      className="tag"
                      style={{ marginLeft: 8, background: CATEGORY_COLORS[r.category] }}
                    >
                      {r.category}
                    </span>
                  )}
                </div>
                {r.window_title && <div className="title">{r.window_title}</div>}
              </div>
              <div className="bar-val">{fmtDuration(r.duration_secs)}</div>
            </div>
          ))
        )}
      </div>
    </>
  );
}

function Screenshots() {
  const [rows, setRows] = useState<ScreenshotRow[]>([]);
  const [imgs, setImgs] = useState<Record<string, string>>({});
  const [zoomIdx, setZoomIdx] = useState<number | null>(null);
  const [from, setFrom] = useState(() => toLocalInput(Date.now() - 14 * 86400_000));
  const [to, setTo] = useState(() => toLocalInput(Date.now() + 86400_000));

  useEffect(() => {
    const f = fromLocalInput(from);
    const t = fromLocalInput(to);
    api
      .screenshots(f, t, 500)
      .then((r) => {
        setRows(r);
        setImgs({});
        r.forEach((s) => {
          api.readScreenshot(s.file_name).then((data) =>
            setImgs((m) => ({ ...m, [s.file_name]: data }))
          );
        });
      })
      .catch(() => setRows([]));
  }, [from, to]);

  // Lightbox keyboard: ESC closes, ←/→ move to previous/next screenshot.
  useEffect(() => {
    if (zoomIdx === null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setZoomIdx(null);
      else if (e.key === "ArrowLeft") setZoomIdx((i) => (i === null ? i : Math.max(0, i - 1)));
      else if (e.key === "ArrowRight")
        setZoomIdx((i) => (i === null ? i : Math.min(rows.length - 1, i + 1)));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [zoomIdx, rows.length]);

  return (
    <>
      <h1>Screenshot Timeline</h1>
      <p className="sub">
        Locally stored periodic captures. Click to enlarge; use ← / → to browse, ESC to close.
      </p>
      <div className="controls">
        <label className="range-label">
          From
          <input type="datetime-local" value={from} onChange={(e) => setFrom(e.target.value)} />
        </label>
        <label className="range-label">
          To
          <input type="datetime-local" value={to} onChange={(e) => setTo(e.target.value)} />
        </label>
        <button
          className="btn ghost"
          onClick={() => {
            setFrom(toLocalInput(Date.now() - 14 * 86400_000));
            setTo(toLocalInput(Date.now() + 86400_000));
          }}
        >
          Reset
        </button>
      </div>
      {rows.length === 0 ? (
        <div className="empty">No screenshots captured yet.</div>
      ) : (
        <div className="shots">
          {rows.map((s, i) => (
            <div className="shot" key={s.id} onClick={() => setZoomIdx(i)}>
              {imgs[s.file_name] ? (
                <img src={imgs[s.file_name]} alt={s.file_name} />
              ) : (
                <div style={{ aspectRatio: "16/9", background: "#000" }} />
              )}
              <div className="meta">
                <div>{s.app_name ?? "—"}</div>
                <div className="t">{fmtDateTime(s.captured_at)}</div>
              </div>
            </div>
          ))}
        </div>
      )}
      {zoomIdx !== null && rows[zoomIdx] && (
        <div className="modal-bg" onClick={() => setZoomIdx(null)}>
          <button
            className="lightbox-nav prev"
            disabled={zoomIdx === 0}
            onClick={(e) => {
              e.stopPropagation();
              setZoomIdx((i) => (i === null ? i : Math.max(0, i - 1)));
            }}
            aria-label="Previous screenshot"
          >
            ‹
          </button>
          <div className="lightbox-body" onClick={(e) => e.stopPropagation()}>
            <img src={imgs[rows[zoomIdx].file_name]} alt="screenshot" />
            <div className="lightbox-caption">
              {rows[zoomIdx].app_name ?? "—"} · {fmtDateTime(rows[zoomIdx].captured_at)} ·{" "}
              {zoomIdx + 1}/{rows.length}
            </div>
          </div>
          <button
            className="lightbox-nav next"
            disabled={zoomIdx === rows.length - 1}
            onClick={(e) => {
              e.stopPropagation();
              setZoomIdx((i) => (i === null ? i : Math.min(rows.length - 1, i + 1)));
            }}
            aria-label="Next screenshot"
          >
            ›
          </button>
        </div>
      )}
    </>
  );
}

function Categories() {
  const [apps, setApps] = useState<{ app_name: string; category: string }[]>([]);
  const reload = () => api.listApps().then(setApps).catch(() => setApps([]));
  useEffect(() => {
    reload();
  }, []);

  const change = (app: string, cat: Category) => {
    api.setCategory(app, cat).then(reload);
  };

  return (
    <>
      <h1>Application Categories</h1>
      <p className="sub">Manually classify applications. Categories drive reports and rules.</p>
      <div className="panel">
        {apps.length === 0 ? (
          <div className="empty">No applications recorded yet.</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Application</th>
                <th>Category</th>
              </tr>
            </thead>
            <tbody>
              {apps.map((a) => (
                <tr key={a.app_name}>
                  <td>{a.app_name}</td>
                  <td>
                    <select
                      value={a.category || ""}
                      onChange={(e) => change(a.app_name, e.target.value as Category)}
                    >
                      <option value="" disabled>
                        Uncategorized
                      </option>
                      {CATEGORIES.map((c) => (
                        <option key={c} value={c}>
                          {c}
                        </option>
                      ))}
                    </select>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}

const EMPTY_RULE = {
  id: null as number | null,
  target: "",
  targetType: "app",
  action: "block",
  schedule: "",
  enabled: true,
  enforcementMode: "monitor",
};

function Rules() {
  const [rules, setRules] = useState<Rule[]>([]);
  const [events, setEvents] = useState<RuleEvent[]>([]);
  const [form, setForm] = useState({ ...EMPTY_RULE });

  const reload = () => {
    api.rules().then(setRules).catch(() => setRules([]));
    api.ruleEvents(200).then(setEvents).catch(() => setEvents([]));
  };
  useEffect(() => {
    reload();
  }, []);

  const save = () => {
    if (!form.target.trim()) return;
    api
      .saveRule({
        id: form.id,
        target: form.target.trim(),
        targetType: form.targetType,
        action: form.action,
        schedule: form.schedule.trim() || null,
        enabled: form.enabled,
        enforcementMode: form.enforcementMode,
      })
      .then(() => {
        setForm({ ...EMPTY_RULE });
        reload();
      });
  };

  const edit = (r: Rule) =>
    setForm({
      id: r.id,
      target: r.target,
      targetType: r.targetType,
      action: r.action,
      schedule: r.schedule ?? "",
      enabled: r.enabled,
      enforcementMode: r.enforcementMode,
    });

  // Flip a rule active/inactive directly from the table (re-saves the row).
  const toggleEnabled = (r: Rule) =>
    api
      .saveRule({
        id: r.id,
        target: r.target,
        targetType: r.targetType,
        action: r.action,
        schedule: r.schedule,
        enabled: !r.enabled,
        enforcementMode: r.enforcementMode,
      })
      .then(reload);

  return (
    <>
      <h1>Rules</h1>
      <p className="sub">
        <strong>Effect = Log only</strong> just records a "would be blocked" event.{" "}
        <strong>Effect = Block</strong> actually blocks — apps are terminated, websites are blocked
        via DNS — while the rule is <strong>Active</strong> and its schedule is current (app blocking
        also needs global enforcement on under Settings).
      </p>
      <div className="banner">
        User-level enforcement is a deterrent and is bypassable (Task Manager kill, DNS override,
        VPN/private browsing, new Windows account). Enable it under Settings → Rule Enforcement.
      </div>

      <div className="panel">
        <h2>{form.id ? "Edit Rule" : "New Rule"}</h2>
        <div className="controls">
          <input
            type="text"
            placeholder="Target (e.g. discord.exe, youtube.com, Entertainment)"
            value={form.target}
            style={{ minWidth: 240 }}
            onChange={(e) => setForm({ ...form, target: e.target.value })}
          />
          <select value={form.targetType} onChange={(e) => setForm({ ...form, targetType: e.target.value })}>
            <option value="app">app (process name)</option>
            <option value="category">category (monitor only)</option>
            <option value="website">website (domain, DNS block)</option>
          </select>
          <input
            type="text"
            placeholder="Schedule (e.g. 20:00-07:00)"
            value={form.schedule}
            onChange={(e) => setForm({ ...form, schedule: e.target.value })}
          />
          <select
            value={form.enforcementMode}
            onChange={(e) => setForm({ ...form, enforcementMode: e.target.value })}
            title="What this rule does when it matches"
          >
            <option value="monitor">Effect: Log only</option>
            <option value="enforce">Effect: Block</option>
          </select>
          <label className="switch">
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={(e) => setForm({ ...form, enabled: e.target.checked })}
            />
            <span className="slider" />
          </label>
          <button className="btn" onClick={save}>
            {form.id ? "Update" : "Add Rule"}
          </button>
          {form.id && (
            <button className="btn ghost" onClick={() => setForm({ ...EMPTY_RULE })}>
              Cancel
            </button>
          )}
        </div>
      </div>

      <div className="panel">
        <h2>Configured Rules</h2>
        {rules.length === 0 ? (
          <div className="empty">No rules defined.</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Target</th>
                <th>Type</th>
                <th>Effect</th>
                <th>Schedule</th>
                <th>Status</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {rules.map((r) => (
                <tr key={r.id}>
                  <td>{r.target}</td>
                  <td>{r.targetType}</td>
                  <td>{r.enforcementMode === "enforce" ? "Block" : "Log only"}</td>
                  <td>{r.schedule ?? "—"}</td>
                  <td>
                    <span className={`status-badge ${r.enabled ? "active" : "inactive"}`}>
                      {r.enabled ? "Active" : "Inactive"}
                    </span>
                  </td>
                  <td className="num">
                    <button className="btn ghost" onClick={() => toggleEnabled(r)}>
                      {r.enabled ? "Deactivate" : "Activate"}
                    </button>{" "}
                    <button className="btn ghost" onClick={() => edit(r)}>
                      Edit
                    </button>{" "}
                    <button className="btn danger" onClick={() => api.deleteRule(r.id).then(reload)}>
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <div className="panel">
        <h2>"Would Be Blocked" Events</h2>
        {events.length === 0 ? (
          <div className="empty">No rule events logged yet.</div>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Time</th>
                <th>App</th>
                <th>Detail</th>
              </tr>
            </thead>
            <tbody>
              {events.map((e) => (
                <tr key={e.id}>
                  <td>{fmtDateTime(e.occurred_at)}</td>
                  <td>{e.app_name ?? "—"}</td>
                  <td>{e.message ?? ""}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}

const SETTING_META: Record<string, { label: string; hint: string }> = {
  screenshot_interval_secs: { label: "Screenshot interval (seconds)", hint: "How often to capture." },
  screenshot_retention_days: { label: "Screenshot retention (days)", hint: "Older screenshots are deleted." },
  idle_threshold_secs: { label: "Idle threshold (seconds)", hint: "No input for this long = idle." },
};

function Settings({ pw, onMonitor }: { pw: string; onMonitor: () => void }) {
  const [settings, setSettings] = useState<SettingRow[]>([]);
  const [autostart, setAutostart] = useState(false);
  const [status, setStatus] = useState<MonitorStatus | null>(null);
  const [dnsBusy, setDnsBusy] = useState(false);

  const reload = () => {
    api.settings().then(setSettings).catch(() => {});
    api.autostart().then(setAutostart).catch(() => {});
    api.monitorStatus().then(setStatus).catch(() => {});
  };
  useEffect(() => {
    reload();
  }, []);

  const get = (k: string) => settings.find((s) => s.key === k)?.value ?? "";
  const update = (k: string, v: string) => {
    api.updateSetting(k, v).then(reload);
  };
  const toggle = (k: string, on: boolean) => update(k, on ? "1" : "0");

  return (
    <>
      <h1>Settings</h1>
      <p className="sub">Configure monitoring behavior. All data stays local.</p>

      <div className="panel">
        <h2>Background Monitor</h2>
        <div className="setting-row">
          <div className="k">
            Status
            <small>
              {status?.running
                ? `Running — last heartbeat ${status.seconds_since_heartbeat}s ago`
                : "Not running"}
            </small>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button
              className="btn"
              onClick={() =>
                api.startMonitor().then(() => {
                  onMonitor();
                  setTimeout(reload, 800);
                })
              }
            >
              Start
            </button>
            <button
              className="btn ghost"
              onClick={() =>
                api
                  .stopMonitor(pw)
                  .then(() => {
                    onMonitor();
                    setTimeout(reload, 800);
                  })
                  .catch((e) => alert(String(e)))
              }
            >
              Stop
            </button>
          </div>
        </div>
        <div className="setting-row">
          <div className="k">
            Start automatically at login
            <small>Registers a user-level Run key (no admin rights).</small>
          </div>
          <label className="switch">
            <input
              type="checkbox"
              checked={autostart}
              onChange={(e) => api.setAutostart(e.target.checked).then(reload)}
            />
            <span className="slider" />
          </label>
        </div>
      </div>

      <div className="panel">
        <h2>Rule Enforcement</h2>
        <div className="banner">
          User-level enforcement is a deterrent. It is bypassable (Task Manager kill, DNS override,
          private browsing/VPN, or a new Windows account). It is not hardened service-based
          protection.
        </div>
        <div className="setting-row">
          <div className="k">
            Enforce app block rules
            <small>
              Terminates processes for app rules in "enforce" mode. Off = app rules are log-only.
              (Website blocking is controlled separately below.)
            </small>
          </div>
          <label className="switch">
            <input
              type="checkbox"
              checked={get("enforcement_enabled") === "1"}
              onChange={(e) => toggle("enforcement_enabled", e.target.checked)}
            />
            <span className="slider" />
          </label>
        </div>
        <div className="setting-row">
          <div className="k">
            Website DNS filtering
            <small>
              {get("dns_filter_enabled") === "1"
                ? "Active — system DNS points only at Nivara's proxy (127.0.0.1); enabled website rules are blocked. If a blocked site still opens, turn off the browser's \"Secure DNS\"/DoH."
                : "Blocks enabled website rules via Nivara's local DNS proxy (127.0.0.1 only, so blocking is airtight). One-time admin (UAC) approval; reversible. Requires the background monitor to be running."}
            </small>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            {get("dns_filter_enabled") === "1" ? (
              <button
                className="btn ghost"
                disabled={dnsBusy}
                onClick={() => {
                  setDnsBusy(true);
                  api.dnsFilterRemove().then(reload).catch((e) => alert(String(e))).finally(() => setDnsBusy(false));
                }}
              >
                Disable DNS filtering
              </button>
            ) : (
              <button
                className="btn"
                disabled={dnsBusy}
                onClick={() => {
                  setDnsBusy(true);
                  api.dnsFilterApply().then(reload).catch((e) => alert(String(e))).finally(() => setDnsBusy(false));
                }}
              >
                Enable DNS filtering
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="panel">
        <h2>Screenshots</h2>
        <div className="setting-row">
          <div className="k">
            Enable screenshot capture
            <small>Local storage only.</small>
          </div>
          <label className="switch">
            <input
              type="checkbox"
              checked={get("screenshots_enabled") === "1"}
              onChange={(e) => toggle("screenshots_enabled", e.target.checked)}
            />
            <span className="slider" />
          </label>
        </div>
        {["screenshot_interval_secs", "screenshot_retention_days", "idle_threshold_secs"].map((k) => (
          <div className="setting-row" key={k}>
            <div className="k">
              {SETTING_META[k].label}
              <small>{SETTING_META[k].hint}</small>
            </div>
            <input
              type="number"
              style={{ width: 100 }}
              defaultValue={get(k)}
              key={`${k}-${get(k)}`}
              onBlur={(e) => update(k, e.target.value)}
            />
          </div>
        ))}
      </div>
    </>
  );
}
