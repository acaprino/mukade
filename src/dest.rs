//! The destination facts a harness states about itself.
//!
//! Two writers need a value only the destination's own configuration knows: OpenCode refuses a
//! model reference without a provider, and Codex stamps the provider its `config.toml` names.
//! The CLI used to read them privately; the TUI needs the same answer without a second copy.

use crate::model::HarnessId;
use crate::roots::StoreRoots;

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
    crate::harnesses::get(harness)
        .destination()
        .and_then(|destination| destination.configured_model(roots))
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
