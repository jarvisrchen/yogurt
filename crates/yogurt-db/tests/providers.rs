use yogurt_db::{providers, Db};

#[test]
fn it_inserts_and_lists_a_provider() {
    let db = Db::open_in_memory().unwrap();
    let id = providers::insert(
        &db,
        providers::NewProvider {
            name: "Minimax".into(),
            base_url: "https://api.minimax.io/v1".into(),
            model: "MiniMax-Text-01".into(),
            adapter: providers::adapter::HTTP.into(),
        },
    )
    .unwrap();
    let rows = providers::list(&db).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    assert_eq!(rows[0].name, "Minimax");
    assert!(!rows[0].is_active);
}

#[test]
fn it_sets_only_one_active_provider() {
    let db = Db::open_in_memory().unwrap();
    let a = providers::insert(
        &db,
        providers::NewProvider {
            name: "A".into(),
            base_url: "https://a/v1".into(),
            model: "m".into(),
            adapter: providers::adapter::HTTP.into(),
        },
    )
    .unwrap();
    let b = providers::insert(
        &db,
        providers::NewProvider {
            name: "B".into(),
            base_url: "https://b/v1".into(),
            model: "m".into(),
            adapter: providers::adapter::HTTP.into(),
        },
    )
    .unwrap();

    providers::set_active(&db, &a).unwrap();
    assert_eq!(providers::active(&db).unwrap().unwrap().id, a);

    providers::set_active(&db, &b).unwrap();
    let active = providers::active(&db).unwrap().unwrap();
    assert_eq!(active.id, b);

    let all = providers::list(&db).unwrap();
    let active_count = all.iter().filter(|p| p.is_active).count();
    assert_eq!(active_count, 1, "exactly one provider should be active");
}

#[test]
fn it_exposes_presets_as_a_const_slice() {
    let names: Vec<&str> = providers::PRESETS.iter().map(|p| p.name).collect();
    assert!(names.contains(&"Minimax"));
    assert!(names.contains(&"OpenAI"));
    assert!(names.contains(&"Ollama (local)"));
    assert!(names.contains(&"LM Studio (local)"));
    assert!(names.contains(&"OpenRouter"));
    assert!(names.contains(&"Google Gemini"));
    assert!(names.contains(&"DeepSeek"));
    assert!(names.contains(&"Claude Code (local CLI)"));
    assert!(names.contains(&"Cursor Agent (local CLI)"));
}

/// Every `adapter: "http"` preset must be a usable base URL for
/// `OpenAiCompatClient`, which builds its endpoint as
/// `{base_url}/chat/completions`. `adapter: "cli"` presets are exempt -
/// `base_url` is unused for them (see `providers::adapter`).
///
/// The trap this guards: Google documents its OpenAI shim WITH a trailing
/// slash (`.../v1beta/openai/`). Pasting that verbatim into a preset yields
/// `.../openai//chat/completions` for anyone reading `base_url` without
/// going through the client's trimming constructor.
#[test]
fn preset_base_urls_are_absolute_and_have_no_trailing_slash() {
    for p in providers::PRESETS {
        assert!(!p.name.trim().is_empty(), "preset name must not be empty");
        if p.adapter == providers::adapter::CLI {
            assert!(
                p.base_url.is_empty(),
                "{}: cli preset must leave base_url empty, got {:?}",
                p.name,
                p.base_url
            );
            continue;
        }
        assert!(
            p.base_url.starts_with("http://") || p.base_url.starts_with("https://"),
            "{} base_url must be absolute, got {:?}",
            p.name,
            p.base_url
        );
        assert!(
            !p.base_url.ends_with('/'),
            "{} base_url must not end in a slash, got {:?}",
            p.name,
            p.base_url
        );
    }
}

/// Each `adapter: "http"` preset's `default_model` is what gets written
/// into the providers table on clone, so it MUST appear in the preset's
/// `models` datalist - otherwise the initial state of the dropdown
/// wouldn't match what was saved and the user would see "Saved as X,
/// suggested Y" confusion. `adapter: "cli"` presets have no model list at
/// all (`default_model` is a `CliProgram` id, not a model name).
#[test]
fn preset_default_model_appears_in_its_models_list() {
    for p in providers::PRESETS {
        if p.adapter == providers::adapter::CLI {
            assert!(
                p.models.is_empty(),
                "{}: cli preset must not have a model list",
                p.name
            );
            continue;
        }
        if p.default_model.is_empty() {
            // LM Studio starts blank; nothing to assert.
            continue;
        }
        assert!(
            p.models.contains(&p.default_model),
            "{}: default_model {:?} must be in models {:?}",
            p.name,
            p.default_model,
            p.models
        );
    }
}

/// Every `adapter: "cli"` preset's `default_model` must be a program id
/// `yogurt_llm::CliProgram::parse` recognizes - if it doesn't,
/// `from_active_provider` would reject a freshly-cloned, never-edited
/// provider row as having an "unrecognized CLI program", which should be
/// impossible. Spelled out as a literal list rather than depending on
/// `yogurt_llm` from this crate just to call `parse` - `yogurt-db` sits
/// below the LLM-client layer in the dependency graph and should stay
/// there; keep this list in sync with `CliProgram::parse` by hand.
#[test]
fn cli_preset_default_models_parse_as_a_known_cli_program() {
    const KNOWN_CLI_PROGRAM_IDS: &[&str] = &["claude", "cursor-agent"];
    for p in providers::PRESETS {
        if p.adapter != providers::adapter::CLI {
            continue;
        }
        assert!(
            KNOWN_CLI_PROGRAM_IDS.contains(&p.default_model),
            "{}: default_model {:?} must be one of {:?}",
            p.name,
            p.default_model,
            KNOWN_CLI_PROGRAM_IDS
        );
    }
}

#[test]
fn list_names_returns_creation_order() {
    let db = Db::open_in_memory().unwrap();
    providers::insert(
        &db,
        providers::NewProvider {
            name: "First".into(),
            base_url: "https://x/v1".into(),
            model: "m".into(),
            adapter: providers::adapter::HTTP.into(),
        },
    )
    .unwrap();
    providers::insert(
        &db,
        providers::NewProvider {
            name: "Second".into(),
            base_url: "https://y/v1".into(),
            model: "m".into(),
            adapter: providers::adapter::HTTP.into(),
        },
    )
    .unwrap();
    let names = providers::list_names(&db).unwrap();
    assert_eq!(names, vec!["First".to_string(), "Second".to_string()]);
}
