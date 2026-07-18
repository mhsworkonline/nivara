// Nivara background monitoring agent.
// Runs as a normal user-level process (launched via HKCU Run key or manually).
// No window, no elevation, no service. Observational only.

#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use nivara_lib::monitor;

fn main() {
    monitor::run_loop();
}
