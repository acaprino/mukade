//! Editable intent prompts: the defaults are copied beside the executable, a file already there is
//! never replaced, and every `.md` file in the folder is an intent whose name is the file stem.

mod support;

use mukade::intent;
use mukade::model::HarnessId;
use std::path::Path;

fn folder(tag: &str) -> std::path::PathBuf {
    support::scratch(HarnessId::Pi, tag).join("intent-prompts")
}

#[test]
fn defaults_are_installed_and_an_edited_prompt_survives() {
    let dir = folder("intent-defaults");
    intent::install_defaults_in(&dir).expect("install the defaults");
    let names: Vec<String> = intent::list_in(&dir).into_iter().map(|e| e.name).collect();
    for expected in [
        "generic",
        "continue",
        "execute",
        "review",
        "analyze",
        "brainstorm",
        "plan",
    ] {
        assert!(names.contains(&expected.to_string()), "{names:?}");
    }
    assert!(
        intent::list_in(&dir)
            .iter()
            .filter(|entry| entry.builtin)
            .all(|entry| entry.custom)
    );

    let generic = dir.join("generic.md");
    std::fs::write(&generic, "My edited generic prompt.").unwrap();
    intent::install_defaults_in(&dir).expect("a later start");
    assert_eq!(
        std::fs::read_to_string(&generic).unwrap(),
        "My edited generic prompt."
    );
    let resolved = intent::resolve_in(&dir, None).expect("the edited generic prompt");
    assert_eq!(resolved.name, "generic");
    assert_eq!(resolved.text, "My edited generic prompt.");
}

#[test]
fn a_new_markdown_file_is_a_new_intent() {
    let dir = folder("intent-custom");
    intent::install_defaults_in(&dir).unwrap();
    std::fs::write(dir.join("security.md"), "Check every input boundary.").unwrap();
    let resolved = intent::resolve_in(&dir, Some("security")).expect("the custom intent");
    assert_eq!(resolved.name, "security");
    assert_eq!(resolved.text, "Check every input boundary.");
    assert!(
        intent::list_in(&dir)
            .iter()
            .any(|entry| entry.name == "security" && !entry.builtin && entry.custom)
    );
}

/// Every shipped prompt must survive a Windows `.cmd` launcher: one line of the safe subset.
#[test]
fn every_default_prompt_is_safe_for_a_batch_launcher() {
    let dir = folder("intent-batch-safe");
    intent::install_defaults_in(&dir).unwrap();
    for name in [
        "generic",
        "continue",
        "execute",
        "review",
        "analyze",
        "brainstorm",
        "plan",
    ] {
        let prompt = intent::resolve_in(&dir, Some(name)).unwrap();
        assert!(!prompt.text.starts_with('-'), "{name}");
        assert!(!prompt.text.contains(['\n', '\r']), "{name}");
        assert!(
            prompt
                .text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || " .,;:()".contains(c)),
            "{name}: {}",
            prompt.text
        );
    }
}

#[test]
fn a_markdown_extension_in_any_case_is_found() {
    let dir = folder("intent-extension-case");
    intent::install_defaults_in(&dir).unwrap();
    std::fs::write(dir.join("Security.MD"), "Check every input boundary.").unwrap();
    let resolved = intent::resolve_in(&dir, Some("Security")).unwrap();
    assert_eq!(resolved.text, "Check every input boundary.");
}

#[test]
fn markdown_whitespace_reaches_the_model_on_one_line() {
    let dir = folder("intent-one-line");
    intent::install_defaults_in(&dir).unwrap();
    std::fs::write(
        dir.join("notes.md"),
        "# Handoff\n\nKeep  the\nuser goal and the open questions.\n",
    )
    .unwrap();
    let resolved = intent::resolve_in(&dir, Some("notes")).unwrap();
    assert_eq!(
        resolved.text,
        "# Handoff Keep the user goal and the open questions."
    );
    assert!(!resolved.text.contains(['\n', '\r']));
}

#[test]
fn a_missing_or_invalid_intent_is_refused_with_the_available_names() {
    let dir = folder("intent-errors");
    intent::install_defaults_in(&dir).unwrap();
    let error = intent::resolve_in(&dir, Some("missing")).unwrap_err();
    assert!(error.contains("unknown intent missing"), "{error}");
    assert!(error.contains("review"), "{error}");
    for invalid in ["", "../review", "review.md", "."] {
        assert!(
            intent::resolve_in(&dir, Some(invalid)).is_err(),
            "{invalid:?} was accepted"
        );
    }
    std::fs::write(dir.join("empty.md"), " \n\n").unwrap();
    let error = intent::resolve_in(&dir, Some("empty")).unwrap_err();
    assert!(error.contains("is empty"), "{error}");
    assert!(Path::new(&dir).is_dir());
}
