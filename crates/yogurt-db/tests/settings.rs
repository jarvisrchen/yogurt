use yogurt_db::{settings, Db};

#[test]
fn it_returns_seeded_defaults() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(
        settings::get(&db, "general.port").unwrap().as_deref(),
        Some("7878")
    );
    assert_eq!(
        settings::get(&db, "general.open_browser_on_start")
            .unwrap()
            .as_deref(),
        Some("true")
    );
}

#[test]
fn it_upserts_a_value() {
    let db = Db::open_in_memory().unwrap();
    settings::set(&db, "general.port", "9000").unwrap();
    assert_eq!(
        settings::get(&db, "general.port").unwrap().as_deref(),
        Some("9000")
    );
    settings::set(&db, "general.port", "9001").unwrap();
    assert_eq!(
        settings::get(&db, "general.port").unwrap().as_deref(),
        Some("9001")
    );
}

#[test]
fn it_loads_typed_general_struct() {
    let db = Db::open_in_memory().unwrap();
    let g = settings::load_general(&db).unwrap();
    assert_eq!(g.port, 7878);
    assert!(g.open_browser_on_start);
    assert_eq!(g.audio_input_device, "");
    // V003 seeds first_run_completed = "false" — onboarding (§5.10) gates on this.
    assert!(!g.first_run_completed);
}

#[test]
fn it_flips_first_run_completed_via_patch() {
    let db = Db::open_in_memory().unwrap();
    let patched = settings::save_general_patch(
        &db,
        settings::GeneralPatch {
            first_run_completed: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(patched.first_run_completed);
    // And it survives a reload.
    assert!(settings::load_general(&db).unwrap().first_run_completed);
}

#[test]
fn it_saves_a_general_patch_and_returns_updated() {
    let db = Db::open_in_memory().unwrap();
    let patched = settings::save_general_patch(
        &db,
        settings::GeneralPatch {
            port: Some(8080),
            open_browser_on_start: Some(false),
            audio_input_device: Some("MacBook Pro Microphone".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(patched.port, 8080);
    assert!(!patched.open_browser_on_start);
    assert_eq!(patched.audio_input_device, "MacBook Pro Microphone");
}

/// Phase 8 (Plan 08-03): V005 seeds defaults; load_general returns them
/// on a fresh DB.
#[test]
fn it_loads_v005_seeded_stt_defaults() {
    let db = Db::open_in_memory().unwrap();
    let g = settings::load_general(&db).unwrap();
    assert_eq!(g.stt_provider, "cloud");
    assert_eq!(g.stt_model, "small.en");
}

/// Phase 8 (Plan 08-03): patch updates both keys.
#[test]
fn it_patches_stt_provider_and_model() {
    let db = Db::open_in_memory().unwrap();
    let patched = settings::save_general_patch(
        &db,
        settings::GeneralPatch {
            stt_provider: Some("local".into()),
            stt_model: Some("medium.en".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(patched.stt_provider, "local");
    assert_eq!(patched.stt_model, "medium.en");
    // Survives reload.
    let g = settings::load_general(&db).unwrap();
    assert_eq!(g.stt_provider, "local");
    assert_eq!(g.stt_model, "medium.en");
}

/// AUD-11: defaults before any patch - echo off, default output device,
/// 512-frame buffer.
#[test]
fn it_loads_echo_defaults() {
    let db = Db::open_in_memory().unwrap();
    let g = settings::load_general(&db).unwrap();
    assert_eq!(g.audio_echo_output_device, "");
    assert!(!g.audio_echo_enabled);
    assert_eq!(g.audio_echo_buffer, 512);
}

/// AUD-11: the three echo fields round-trip through a patch + reload.
#[test]
fn it_round_trips_echo_settings() {
    let db = Db::open_in_memory().unwrap();
    let patched = settings::save_general_patch(
        &db,
        settings::GeneralPatch {
            audio_echo_output_device: Some("BlackHole 2ch".into()),
            audio_echo_enabled: Some(true),
            audio_echo_buffer: Some(1024),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(patched.audio_echo_output_device, "BlackHole 2ch");
    assert!(patched.audio_echo_enabled);
    assert_eq!(patched.audio_echo_buffer, 1024);

    let g = settings::load_general(&db).unwrap();
    assert_eq!(g.audio_echo_output_device, "BlackHole 2ch");
    assert!(g.audio_echo_enabled);
    assert_eq!(g.audio_echo_buffer, 1024);
}
