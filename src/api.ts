import { invoke } from "@tauri-apps/api/core";

export type Category = "Study" | "Social" | "Entertainment" | "Other";

export interface ActivityRow {
  id: number;
  app_name: string;
  window_title: string;
  started_at: number;
  ended_at: number;
  duration_secs: number;
  is_idle: boolean;
  category: string | null;
}

export interface UsageRow {
  app_name: string;
  category: string | null;
  total_secs: number;
}

export interface CategoryTotal {
  category: string;
  total_secs: number;
}

export interface Report {
  from: number;
  to: number;
  total_active_secs: number;
  idle_secs: number;
  by_category: CategoryTotal[];
  top_apps: UsageRow[];
}

export interface ScreenshotRow {
  id: number;
  file_name: string;
  captured_at: number;
  app_name: string | null;
  window_title: string | null;
}

export interface CategoryRow {
  app_name: string;
  category: string;
}

export interface Rule {
  id: number;
  target: string;
  targetType: string;
  action: string;
  schedule: string | null;
  enabled: boolean;
  enforcementMode: string;
}

export interface RuleEvent {
  id: number;
  rule_id: number;
  app_name: string | null;
  window_title: string | null;
  occurred_at: number;
  message: string | null;
}

export interface SettingRow {
  key: string;
  value: string;
}

export interface MonitorStatus {
  running: boolean;
  heartbeat: number;
  seconds_since_heartbeat: number;
  should_run: boolean;
}

export const api = {
  activityTimeline: (from: number, to: number, limit = 500) =>
    invoke<ActivityRow[]>("get_activity_timeline", { from, to, limit }),
  usageSummary: (from: number, to: number) =>
    invoke<UsageRow[]>("get_usage_summary", { from, to }),
  dailyReport: (dayStart: number) =>
    invoke<Report>("get_daily_report", { dayStart }),
  weeklyReport: (weekStart: number) =>
    invoke<Report>("get_weekly_report", { weekStart }),
  screenshots: (from: number, to: number, limit = 500) =>
    invoke<ScreenshotRow[]>("get_screenshots", { from, to, limit }),
  readScreenshot: (fileName: string) =>
    invoke<string>("read_screenshot", { fileName }),
  categories: () => invoke<CategoryRow[]>("get_categories"),
  setCategory: (appName: string, category: Category) =>
    invoke<void>("set_category", { appName, category }),
  listApps: () => invoke<CategoryRow[]>("list_apps"),
  rules: () => invoke<Rule[]>("get_rules"),
  saveRule: (r: {
    id: number | null;
    target: string;
    targetType: string;
    action: string;
    schedule: string | null;
    enabled: boolean;
    enforcementMode: string;
  }) => invoke<number>("save_rule", r),
  deleteRule: (id: number) => invoke<void>("delete_rule", { id }),
  ruleEvents: (limit = 200) =>
    invoke<RuleEvent[]>("get_rule_events", { limit }),
  settings: () => invoke<SettingRow[]>("get_settings"),
  updateSetting: (key: string, value: string) =>
    invoke<void>("update_setting", { key, value }),
  monitorStatus: () => invoke<MonitorStatus>("get_monitor_status"),
  startMonitor: () => invoke<void>("start_monitor"),
  stopMonitor: (password: string) => invoke<void>("stop_monitor", { password }),
  autostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) =>
    invoke<void>("set_autostart", { enabled }),
  // Password gate
  authStatus: () => invoke<boolean>("auth_status"),
  authSetup: (password: string) => invoke<string>("auth_setup", { password }),
  authVerify: (password: string) => invoke<boolean>("auth_verify", { password }),
  authRecover: (recoveryCode: string, newPassword: string) =>
    invoke<void>("auth_recover", { recoveryCode, newPassword }),
  // DNS website filtering (elevated apply/remove)
  dnsFilterApply: () => invoke<void>("dns_filter_apply"),
  dnsFilterRemove: () => invoke<void>("dns_filter_remove"),
  // Screen time + restrictions
  screentimeStatus: () => invoke<ScreenTimeStatus>("screentime_status"),
  screentimeGrant: (minutes: number) =>
    invoke<void>("screentime_grant", { minutes }),
  screentimeResetToday: () => invoke<void>("screentime_reset_today"),
  restrictionsApply: (enabled: boolean) =>
    invoke<void>("restrictions_apply", { enabled }),
};

export interface ScreenTimeStatus {
  enabled: boolean;
  used_secs: number;
  allowance_secs: number | null;
  remaining_secs: number | null;
  bonus_secs: number;
  in_bedtime: boolean;
  over: boolean;
}

export function fmtDuration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

export function fmtTime(epoch: number): string {
  return new Date(epoch * 1000).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function fmtDateTime(epoch: number): string {
  return new Date(epoch * 1000).toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Epoch seconds (ms input) -> "YYYY-MM-DDTHH:MM" for <input type=datetime-local>. */
export function toLocalInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours()
  )}:${pad(d.getMinutes())}`;
}

/** datetime-local string -> epoch seconds. */
export function fromLocalInput(s: string): number {
  const t = new Date(s).getTime();
  return Math.floor((isNaN(t) ? Date.now() : t) / 1000);
}

export function startOfDay(d: Date): number {
  const c = new Date(d);
  c.setHours(0, 0, 0, 0);
  return Math.floor(c.getTime() / 1000);
}

export function startOfWeek(d: Date): number {
  const c = new Date(d);
  const day = c.getDay();
  const diff = (day + 6) % 7; // Monday start
  c.setDate(c.getDate() - diff);
  c.setHours(0, 0, 0, 0);
  return Math.floor(c.getTime() / 1000);
}

export const CATEGORY_COLORS: Record<string, string> = {
  Study: "#3b82f6",
  Social: "#ec4899",
  Entertainment: "#f59e0b",
  Other: "#64748b",
};
