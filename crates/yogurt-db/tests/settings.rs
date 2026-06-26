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
        },
    )
    .unwrap();
    assert_eq!(patched.port, 8080);
    assert!(!patched.open_browser_on_start);
    assert_eq!(patched.audio_input_device, "MacBook Pro Microphone");
}
