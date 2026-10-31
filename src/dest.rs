//! The destination facts a harness states about itself.
//!
//! Two writers need a value only the destination's own configuration knows: OpenCode refuses a
//! model reference without a provider, and Codex stamps the provider its `config.toml` names.
//! The CLI used to read them privately; the TUI needs the same answer without a second copy.

use crate::model::HarnessId;
use crate::roots::StoreRoots;
use std::path::{Path, PathBuf};

/// The provider and model the destination's own configuration names, when it names them, for the
/// stores this process's variables name. See [`configured_model_in`].
pub fn configured_model(harness: HarnessId) -> Option<(String, Option<String>)> {
    configured_model_in(harness, &StoreRoots::from_env())
}

/// The provider and model the configuration of the destination `roots` name states. OpenCode's
/// `model` is `<provider>/<model>`; Codex states `model_provider` at the top level.
///
/// A self-contained installation keeps its configuration beside its store: an OpenCode database
/// in an XDG data layout (`<root>/share/opencode/opencode.db`, a portable JanuraCode's) is
/// configured by `<root>/config/opencode/opencode.json`, a Codex home by its `config.toml`. When
/// such a file exists it alone answers, even without a model: another installation's model is
/// not this one's. Otherwise OpenCode's process configuration applies (`OPENCODE_CONFIG`, then
/// `XDG_CONFIG_HOME`).
pub fn configured_model_in(
    harness: HarnessId,
    roots: &StoreRoots,
) -> Option<(String, Option<String>)> {
    match harness {
        HarnessId::OpenCode => {
            let own = roots
                .opencode_db
                .as_deref()
                .and_then(opencode_config_beside)
                .filter(|path| path.is_file());
            opencode_model(&own.or_else(opencode_process_config)?)
        }
        HarnessId::Codex => {
            let home = roots.codex_home.as_deref()?;
            codex_provider(&home.join("config.toml")).map(|provider| (provider, None))
        }
        HarnessId::ClaudeCode | HarnessId::Pi => None,
    }
}

/// The configuration an OpenCode database in an XDG data layout has beside it:
/// `<root>/share/opencode/opencode.db` → `<root>/config/opencode/opencode.json`.
fn opencode_config_beside(database: &Path) -> Option<PathBuf> {
    let opencode = database.parent()?;
    let share = opencode.parent()?;
    let named = |path: &Path, name: &str| path.file_name().is_some_and(|n| n == name);
    (named(opencode, "opencode") && named(share, "share"))
        .then(|| share.parent())
        .flatten()
        .map(|root| root.join("config").join("opencode").join("opencode.json"))
}

/// The configuration this process's OpenCode reads.
fn opencode_process_config() -> Option<PathBuf> {
    std::env::var_os("OPENCODE_CONFIG")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(|dir| PathBuf::from(dir).join("opencode").join("opencode.json"))
        })
}

/// OpenCode's config names its model as `<provider>/<model>`.
fn opencode_model(config: &Path) -> Option<(String, Option<String>)> {
    let document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config).ok()?).ok()?;
    let (provider, model) = document.get("model")?.as_str()?.split_once('/')?;
    Some((provider.to_string(), Some(model.to_string())))
}

/// Codex's `config.toml` states `model_provider = "<id>"`.
fn codex_provider(config: &Path) -> Option<String> {
    let text = std::fs::read_to_string(config).ok()?;
    top_level_string(&text, "model_provider")
}

/// A top-level `key = "value"` of a TOML file: before any `[table]`, the key exactly (not
/// `model_providers.x`), with a trailing `# comment` left out.
fn top_level_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            return None;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        let value = match value.strip_prefix('"') {
            Some(quoted) => quoted.split('"').next().unwrap_or_default(),
            None => value.split('#').next().unwrap_or_default().trim(),
        };
        return Some(value.to_string()).filter(|value| !value.is_empty());
    }
    None
}

#[cfg(test)]
mod installation_tests {
    use super::*;
    use crate::roots::StoreRoots;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mukade-dest-{tag}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A portable JanuraCode keeps its OpenCode configuration beside its data: the model it
    /// names is the destination's, whatever the shell's own OpenCode uses.
    #[test]
    fn a_self_contained_opencode_names_its_model_beside_its_data() {
        let data = scratch("opencode").join("JanuraCode/data");
        write(
            &data.join("config/opencode/opencode.json"),
            r#"{"model":"tri-llm-gateway/tritech-auto"}"#,
        );
        let roots = StoreRoots {
            opencode_db: Some(data.join("share/opencode/opencode.db")),
            ..Default::default()
        };
        assert_eq!(
            configured_model_in(HarnessId::OpenCode, &roots),
            Some((
                "tri-llm-gateway".to_string(),
                Some("tritech-auto".to_string())
            ))
        );
    }

    #[test]
    fn a_configuration_without_a_model_names_none_rather_than_another_installations() {
        let data = scratch("opencode-no-model").join("data");
        write(&data.join("config/opencode/opencode.json"), "{}");
        let roots = StoreRoots {
            opencode_db: Some(data.join("share/opencode/opencode.db")),
            ..Default::default()
        };
        assert_eq!(configured_model_in(HarnessId::OpenCode, &roots), None);
    }

    #[test]
    fn only_an_xdg_data_layout_has_a_configuration_beside_it() {
        let root = scratch("opencode-layout");
        assert_eq!(
            opencode_config_beside(&root.join("data/share/opencode/opencode.db")),
            Some(root.join("data/config/opencode/opencode.json"))
        );
        assert_eq!(
            opencode_config_beside(&root.join("store/opencode.db")),
            None
        );
    }

    #[test]
    fn a_codex_home_names_its_provider_in_its_own_config() {
        let home = scratch("codex");
        write(&home.join("config.toml"), "model_provider = \"azure\"\n");
        let roots = StoreRoots {
            codex_home: Some(home),
            ..Default::default()
        };
        assert_eq!(
            configured_model_in(HarnessId::Codex, &roots),
            Some(("azure".to_string(), None))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::top_level_string;

    /// Codex's own provider is the top-level key: not a provider table's field, not a profile's,
    /// and not the comment after it.
    #[test]
    fn the_codex_provider_is_the_top_level_key_only() {
        let config = "model = \"gpt\"\nmodel_provider = \"azure\" # the work account\n\n[model_providers.azure]\nname = \"Azure\"\n";
        assert_eq!(
            top_level_string(config, "model_provider").as_deref(),
            Some("azure")
        );
        let nested = "[profiles.fast]\nmodel_provider = \"openai\"\n";
        assert_eq!(top_level_string(nested, "model_provider"), None);
        let dotted = "model_providers.azure.name = \"Azure\"\n";
        assert_eq!(top_level_string(dotted, "model_provider"), None);
    }
}
