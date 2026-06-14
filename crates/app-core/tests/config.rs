//! Tests for the configurable idle-timeout setting and its persistence.

use std::time::Duration;

use app_core::testing::FakeClock;
use app_core::{config, AppState, SystemClock};
use tempfile::tempdir;

fn state_at(vault: std::path::PathBuf) -> AppState {
    AppState::new(vault, Duration::from_secs(900), Box::new(SystemClock))
}

#[test]
fn set_idle_timeout_updates_status() {
    let dir = tempdir().unwrap();
    let state = state_at(dir.path().join("vault.spk"));
    let status = state.set_idle_timeout(60).unwrap();
    assert_eq!(status.idle_timeout_secs, 60);
}

#[test]
fn set_idle_timeout_persists_to_config_beside_vault() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault.spk");
    let state = state_at(vault.clone());
    state.set_idle_timeout(120).unwrap();

    let cfg = config::load(&config::config_path_for_vault(&vault));
    assert_eq!(cfg.idle_timeout_secs, 120);
}

#[test]
fn config_load_defaults_when_missing() {
    let dir = tempdir().unwrap();
    let cfg = config::load(&dir.path().join("does-not-exist.json"));
    assert_eq!(cfg.idle_timeout_secs, config::DEFAULT_IDLE_TIMEOUT_SECS);
}

#[test]
fn config_round_trips_through_disk() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.json");
    let cfg = config::AppConfig {
        idle_timeout_secs: 42,
    };
    config::save(&path, &cfg).unwrap();
    assert_eq!(config::load(&path), cfg);
}

#[test]
fn zero_timeout_never_auto_locks() {
    let dir = tempdir().unwrap();
    let clock = FakeClock::new();
    let state = AppState::new(
        dir.path().join("vault.spk"),
        Duration::from_secs(900),
        Box::new(clock.clone()),
    );
    state.create_vault("correct horse battery staple").unwrap();
    state.set_idle_timeout(0).unwrap();

    // Advance far past any normal timeout; with 0 ("never") it must stay open.
    clock.advance(Duration::from_secs(10 * 24 * 60 * 60));
    assert!(
        !state.tick_autolock(),
        "zero timeout should never auto-lock"
    );
    assert!(state.status().unlocked);
}
