//! A view over session metadata; original session indices remain the action targets.

use crate::tree;
use mukade::reader::SessionInfo;
use std::collections::BTreeMap;

pub struct Group {
    pub cwd: Option<String>,
    pub display_path: String,
    sessions: Vec<usize>,
    depths: Vec<usize>,
}

/// The sessions of every directory filter, each child or fork placed under its parent when the
/// filter lists the parent too ([`tree::arrange`]).
pub struct Directories {
    all: Vec<usize>,
    all_depths: Vec<usize>,
    groups: Vec<Group>,
}

impl Directories {
    pub fn new(sessions: &[SessionInfo]) -> Self {
        let mut paths: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut unknown = Vec::new();
        for (index, session) in sessions.iter().enumerate() {
            match session.cwd.as_deref().filter(|cwd| !cwd.trim().is_empty()) {
                Some(cwd) => paths.entry(cwd.to_owned()).or_default().push(index),
                None => unknown.push(index),
            }
        }
        let path_names: Vec<_> = paths.keys().cloned().collect();
        let mut groups: Vec<_> = paths
            .into_iter()
            .map(|(cwd, members)| {
                let (sessions, depths) = arranged(sessions, &members);
                Group {
                    display_path: distinguishing_path(&cwd, &path_names),
                    cwd: Some(cwd),
                    sessions,
                    depths,
                }
            })
            .collect();
        if !unknown.is_empty() {
            let (members, depths) = arranged(sessions, &unknown);
            groups.push(Group {
                cwd: None,
                display_path: String::new(),
                sessions: members,
                depths,
            });
        }
        let (all, all_depths) = arranged(sessions, &(0..sessions.len()).collect::<Vec<_>>());
        Self {
            all,
            all_depths,
            groups,
        }
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// Row zero shows all sessions. The remaining rows correspond to `groups()`.
    pub fn sessions(&self, directory: usize) -> &[usize] {
        if directory == 0 {
            &self.all
        } else {
            self.groups
                .get(directory - 1)
                .map_or(&[], |group| group.sessions.as_slice())
        }
    }

    /// How deep each of [`Directories::sessions`] sits under a listed parent, in the same order.
    pub fn depths(&self, directory: usize) -> &[usize] {
        if directory == 0 {
            &self.all_depths
        } else {
            self.groups
                .get(directory - 1)
                .map_or(&[], |group| group.depths.as_slice())
        }
    }

    pub fn len(&self) -> usize {
        self.groups.len() + 1
    }
}

/// `members` in tree order, with the depth of each.
fn arranged(sessions: &[SessionInfo], members: &[usize]) -> (Vec<usize>, Vec<usize>) {
    tree::arrange(sessions, members)
        .into_iter()
        .map(|place| (place.index, place.depth))
        .unzip()
}

/// Shared leaf names need a distinguishing parent before the path is clipped to a column.
fn distinguishing_path(cwd: &str, paths: &[String]) -> String {
    let peers: Vec<_> = paths
        .iter()
        .filter(|other| other.as_str() != cwd && name(other) == name(cwd))
        .collect();
    if peers.is_empty() {
        return cwd.to_string();
    }
    let starts =
        std::iter::once(0)
            .chain(cwd.char_indices().filter_map(|(index, ch)| {
                matches!(ch, '/' | '\\').then_some(index + ch.len_utf8())
            }))
            .filter(|&index| index < cwd.len())
            .collect::<Vec<_>>();
    for start in starts.into_iter().rev() {
        let suffix = &cwd[start..];
        if peers.iter().all(|other| !other.ends_with(suffix)) {
            return suffix.to_string();
        }
    }
    cwd.to_string()
}

/// Interpret both native separator styles without consulting the filesystem.
pub fn name(cwd: &str) -> &str {
    cwd.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_leaf_names_stay_distinct_and_blank_paths_are_unknown() {
        let sessions: Vec<_> = [Some("C:\\a\\app"), Some("D:\\b\\app"), None, Some("  ")]
            .into_iter()
            .map(|cwd| SessionInfo {
                cwd: cwd.map(String::from),
                ..Default::default()
            })
            .collect();
        let directories = Directories::new(&sessions);
        assert_eq!(directories.len(), 4);
        assert_eq!(directories.sessions(0), [0, 1, 2, 3]);
        assert_eq!(directories.sessions(1), [0]);
        assert_eq!(directories.sessions(2), [1]);
        assert_eq!(directories.sessions(3), [2, 3]);
        assert_eq!(directories.groups()[0].cwd.as_deref(), Some("C:\\a\\app"));
        assert_eq!(directories.groups()[1].cwd.as_deref(), Some("D:\\b\\app"));
        assert!(directories.sessions(99).is_empty());
        assert_eq!(name("C:\\a\\app\\"), "app");
        assert_eq!(name("/work/日本語"), "日本語");
        assert_eq!(name("/"), "/");
    }

    fn listed(id: &str, cwd: &str, parent: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            cwd: Some(cwd.to_string()),
            parent: parent.map(String::from),
            parent_kind: parent.map(|_| mukade::model::ParentKind::fork()),
            ..Default::default()
        }
    }

    #[test]
    fn a_child_follows_its_parent_in_every_filter_that_lists_both() {
        let sessions = [
            listed("kid", "/a", Some("top")),
            listed("top", "/a", None),
            listed("far", "/b", Some("top")),
            listed("lone", "/b", None),
        ];
        let directories = Directories::new(&sessions);
        // All: the parent first, then its two children, then the rest.
        assert_eq!(directories.sessions(0), [1, 0, 2, 3]);
        assert_eq!(directories.depths(0), [0, 1, 1, 0]);
        // /a lists the parent and a child.
        assert_eq!(directories.sessions(1), [1, 0]);
        assert_eq!(directories.depths(1), [0, 1]);
        // /b lists a child whose parent is in /a: it stays at the top, so it is still found.
        assert_eq!(directories.sessions(2), [2, 3]);
        assert_eq!(directories.depths(2), [0, 0]);
        assert!(directories.depths(99).is_empty());
    }
}
