//! The release build's Content-Security-Policy must let the audio library run.
//!
//! Tone.js ticks its transport from a Worker made out of a blob. When the
//! policy refuses that, the worker fails without an exception, nothing that
//! was scheduled ever fires, and the pets dance and play in silence. Only
//! release builds have a policy (the dev server sends none), so this cannot
//! be seen while developing.

#[test]
fn blob_workers_are_allowed() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let csp = conf["app"]["security"]["csp"].as_str().expect("app.security.csp");
    let worker = csp.split(';').map(str::trim).find(|d| d.starts_with("worker-src")).expect("a worker-src directive");
    assert!(worker.split_whitespace().any(|s| s == "blob:"), "worker-src must allow blob: ({worker})");
}
