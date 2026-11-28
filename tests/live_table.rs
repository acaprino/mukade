//! The live runner's table (`tests/live/harnesses.json`) says how `tests/live/run.mjs` keeps each
//! harness inside a folder of its own. It must say what the adapters say: the sandbox variables and
//! store of `Harness::sandbox_install`, and the store variable of the harness catalog. A change to
//! either side that is not made to the other fails here, so the runner cannot test a store the
//! harness no longer uses.

use mukade::model::HarnessId;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn table() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/live/harnesses.json");
    let text = std::fs::read_to_string(path).expect("read tests/live/harnesses.json");
    serde_json::from_str(&text).expect("tests/live/harnesses.json is JSON")
}

fn entries(table: &Value) -> &Vec<Value> {
    table["harnesses"].as_array().expect("a harnesses array")
}

fn text<'a>(value: &'a Value, what: &str) -> &'a str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("{what} is a string"))
}

fn env_of(entry: &Value) -> BTreeMap<String, String> {
    entry["env"]
        .as_object()
        .expect("an env object")
        .iter()
        .map(|(name, path)| (name.clone(), text(path, "an env path").to_string()))
        .collect()
}

#[test]
fn the_runner_covers_every_harness_once() {
    let table = table();
    let ids: Vec<&str> = entries(&table)
        .iter()
        .map(|entry| text(&entry["id"], "id"))
        .collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "a harness is listed twice: {ids:?}"
    );
    let all: BTreeSet<&str> = HarnessId::ALL.iter().map(|id| id.as_str()).collect();
    assert_eq!(unique, all);
    for id in HarnessId::ALL {
        assert!(
            table["real_models"][id.as_str()].is_object(),
            "real_models has no entry for {id}"
        );
    }
}

#[test]
fn the_runner_isolates_each_harness_as_its_adapter_does() {
    let table = table();
    for entry in entries(&table) {
        let id: HarnessId = text(&entry["id"], "id").parse().expect("a harness id");
        let store = &entry["store"];
        let env = env_of(entry);
        assert_eq!(
            text(&store["env"], "store.env"),
            mukade::catalog::store_env(id),
            "{id}: the store variable differs from catalog/harnesses.json"
        );
        let folders: Vec<&str> = store["folders"]
            .as_array()
            .expect("store.folders")
            .iter()
            .map(|folder| text(folder, "a folder"))
            .collect();
        let install = &entry["install"];
        match mukade::harnesses::get(id).sandbox_install() {
            Some(sandbox) => {
                assert_eq!(text(&install["kind"], "install.kind"), "npm", "{id}");
                assert_eq!(
                    text(&install["package"], "install.package"),
                    sandbox.package,
                    "{id}"
                );
                assert_eq!(
                    install["ignore_scripts"].as_bool(),
                    Some(sandbox.ignore_scripts),
                    "{id}"
                );
                let adapter: BTreeMap<String, String> = sandbox
                    .env
                    .iter()
                    .map(|(name, path)| (name.to_string(), path.to_string()))
                    .collect();
                assert_eq!(env, adapter, "{id}: env differs from the adapter's sandbox");
                assert_eq!(text(&store["path"], "store.path"), sandbox.store, "{id}");
                assert_eq!(folders, sandbox.store_folders, "{id}");
            }
            None => {
                // No npm sandbox (Hermes): the runner's install is its own, and the only variable
                // it sets is the store's, pointing at the store.
                assert_ne!(
                    text(&install["kind"], "install.kind"),
                    "npm",
                    "{id}: an npm harness with no adapter sandbox"
                );
                let only_store = BTreeMap::from([(
                    text(&store["env"], "store.env").to_string(),
                    text(&store["path"], "store.path").to_string(),
                )]);
                assert_eq!(env, only_store, "{id}");
            }
        }
    }
}

#[test]
fn every_pinned_version_is_exact() {
    let table = table();
    for entry in entries(&table) {
        let version = text(&entry["install"]["version"], "install.version");
        assert!(
            version.chars().next().is_some_and(|c| c.is_ascii_digit())
                && !version.contains(['^', '~', '*', '>', '<', ' ']),
            "{}: {version:?} is not an exact version",
            entry["id"]
        );
    }
}
