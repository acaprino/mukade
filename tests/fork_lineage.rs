//! How a fork names its origin: the model's `ParentKind::Fork { at }`, and each reader's lineage
//! (design `docs/superpowers/specs/2026-11-12-forks-branches-design.md`, part A).

use mukade::model::ParentKind;

#[test]
fn a_fork_may_say_where_it_left_its_parent_and_the_old_form_still_reads() {
    assert_eq!(ParentKind::fork(), ParentKind::Fork { at: None });
    assert_eq!(ParentKind::fork().name(), "fork");
    // The unit form a dump written before this change holds.
    let old: ParentKind = serde_json::from_str(r#"{"kind":"fork"}"#).unwrap();
    assert_eq!(old, ParentKind::fork());
    // No boundary known: the document is as it was.
    assert_eq!(
        serde_json::to_string(&ParentKind::fork()).unwrap(),
        r#"{"kind":"fork"}"#
    );
    let at = ParentKind::Fork {
        at: Some("entry-7".into()),
    };
    let text = serde_json::to_string(&at).unwrap();
    assert_eq!(text, r#"{"kind":"fork","at":"entry-7"}"#);
    assert_eq!(serde_json::from_str::<ParentKind>(&text).unwrap(), at);
}
