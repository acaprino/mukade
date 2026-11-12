//! Sessions arranged as a tree: each child or fork under its parent, when the parent is listed.
//!
//! The reader's `list_all` names every child with its `parent`; this only orders and describes.
//! Nothing here knows a harness: the readers say what a child is (`parent_kind`).

use mukade::model::ParentKind;
use mukade::reader::SessionInfo;
use std::collections::HashMap;

/// One listed session at its place in the tree: an index into the session list, and how many
/// listed ancestors it hangs under (0 for a root, or for a child whose parent is not listed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub index: usize,
    pub depth: usize,
}

/// `members` (indices into `sessions`, in the order they are listed) with each member that has a
/// listed parent moved under it, children in listed order. Roots keep the listed order, so a
/// listing sorted by time stays sorted by the time of its roots. Every member appears exactly
/// once, whatever the parent links say: a link to a session that is not a member, to itself, or
/// round a cycle leaves that session at the top.
pub fn arrange(sessions: &[SessionInfo], members: &[usize]) -> Vec<Place> {
    let mut by_id: HashMap<&str, usize> = HashMap::new();
    for (position, &index) in members.iter().enumerate() {
        by_id.entry(sessions[index].id.as_str()).or_insert(position);
    }
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); members.len()];
    let mut roots = Vec::new();
    for (position, &index) in members.iter().enumerate() {
        let parent = sessions[index]
            .parent
            .as_deref()
            .and_then(|id| by_id.get(id).copied())
            .filter(|&parent| parent != position);
        match parent {
            Some(parent) => kids[parent].push(position),
            None => roots.push(position),
        }
    }
    let mut seen = vec![false; members.len()];
    let mut out = Vec::with_capacity(members.len());
    // The roots first; then whatever a cycle kept out of every root's subtree, as roots.
    let leftovers = (0..members.len()).collect::<Vec<_>>();
    for start in roots.into_iter().chain(leftovers) {
        let mut stack = vec![(start, 0)];
        while let Some((position, depth)) = stack.pop() {
            if std::mem::replace(&mut seen[position], true) {
                continue;
            }
            out.push(Place {
                index: members[position],
                depth,
            });
            stack.extend(kids[position].iter().rev().map(|&kid| (kid, depth + 1)));
        }
    }
    out
}

/// The mark a child or fork carries in the list: `↳ agent <type>` for a subagent, `⑂ fork` for a
/// fork, a bare `↳` for a parent the source links without saying how. `None` for a root.
pub fn mark(info: &SessionInfo) -> Option<String> {
    info.parent.as_ref()?;
    Some(match &info.parent_kind {
        Some(ParentKind::Spawn {
            agent: Some(agent), ..
        }) => format!("↳ agent {agent}"),
        Some(ParentKind::Spawn { .. }) => "↳ agent".to_string(),
        Some(ParentKind::Fork { .. }) => "⑂ fork".to_string(),
        _ => "↳".to_string(),
    })
}

/// How the session came from its parent, for the detail pane: `subagent · explore · started from
/// the parent's conversation`, `fork · at <entry>`, or `linked, kind not recorded`.
pub fn kind_text(info: &SessionInfo) -> Option<String> {
    info.parent.as_ref()?;
    Some(match &info.parent_kind {
        Some(ParentKind::Spawn {
            agent,
            inherits_context,
            ..
        }) => {
            let mut text = "subagent".to_string();
            if let Some(agent) = agent {
                text.push_str(&format!(" · {agent}"));
            }
            if *inherits_context {
                text.push_str(" · started from the parent's conversation");
            }
            text
        }
        Some(ParentKind::Fork { at }) => match at {
            Some(at) => format!("fork · at {at}"),
            None => "fork".to_string(),
        },
        _ => "linked, kind not recorded".to_string(),
    })
}

/// The listed sessions that name one session as their parent, by how they came from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Children {
    pub subagents: usize,
    pub forks: usize,
    /// Linked to the parent by a relation the readers did not name.
    pub other: usize,
}

impl Children {
    pub fn total(&self) -> usize {
        self.subagents + self.forks + self.other
    }

    /// `3 (2 subagents, 1 fork)`, or `None` when there are none.
    pub fn text(&self) -> Option<String> {
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        let parts: Vec<String> = [
            (self.subagents, "subagent", "subagents"),
            (self.forks, "fork", "forks"),
            (self.other, "linked", "linked"),
        ]
        .into_iter()
        .filter(|(count, ..)| *count > 0)
        .map(|(count, one, many)| plural(count, one, many))
        .collect();
        (!parts.is_empty()).then(|| format!("{} ({})", self.total(), parts.join(", ")))
    }
}

/// The listed sessions whose parent is `id`.
pub fn children_of(sessions: &[SessionInfo], id: &str) -> Children {
    let mut found = Children::default();
    for info in sessions
        .iter()
        .filter(|info| info.parent.as_deref() == Some(id) && info.id != id)
    {
        match info.parent_kind {
            Some(ParentKind::Spawn { .. }) => found.subagents += 1,
            Some(ParentKind::Fork { .. }) => found.forks += 1,
            _ => found.other += 1,
        }
    }
    found
}

/// The indentation and mark that precede a session's title: two cells per level, capped so a deep
/// chain still leaves room for the title.
pub fn prefix(info: &SessionInfo, depth: usize) -> String {
    let indent = "  ".repeat(depth.min(6));
    match mark(info) {
        Some(mark) => format!("{indent}{mark} "),
        None => indent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, parent: Option<(&str, ParentKind)>) -> SessionInfo {
        SessionInfo {
            id: id.to_string(),
            parent: parent.as_ref().map(|(id, _)| id.to_string()),
            parent_kind: parent.map(|(_, kind)| kind),
            ..Default::default()
        }
    }

    fn spawn(agent: Option<&str>) -> ParentKind {
        ParentKind::Spawn {
            call_id: None,
            agent: agent.map(String::from),
            inherits_context: false,
        }
    }

    fn all(sessions: &[SessionInfo]) -> Vec<Place> {
        arrange(sessions, &(0..sessions.len()).collect::<Vec<_>>())
    }

    fn shape(places: &[Place]) -> Vec<(usize, usize)> {
        places.iter().map(|p| (p.index, p.depth)).collect()
    }

    #[test]
    fn a_child_sits_under_its_parent_wherever_the_listing_put_it() {
        let sessions = [
            info("child", Some(("parent", spawn(Some("explore"))))),
            info("other", None),
            info("parent", None),
            info("fork", Some(("parent", ParentKind::fork()))),
        ];
        assert_eq!(
            shape(&all(&sessions)),
            [(1, 0), (2, 0), (0, 1), (3, 1)],
            "roots keep the listed order, the children follow their parent in listed order"
        );
    }

    #[test]
    fn grandchildren_nest_one_level_further() {
        let sessions = [
            info("deep", Some(("mid", spawn(None)))),
            info("mid", Some(("top", spawn(None)))),
            info("top", None),
        ];
        assert_eq!(shape(&all(&sessions)), [(2, 0), (1, 1), (0, 2)]);
    }

    #[test]
    fn a_child_whose_parent_is_not_listed_stays_at_the_top() {
        let sessions = [
            info("orphan", Some(("elsewhere", spawn(None)))),
            info("root", None),
        ];
        assert_eq!(shape(&all(&sessions)), [(0, 0), (1, 0)]);
    }

    #[test]
    fn only_the_members_count_as_listed() {
        let sessions = [
            info("parent", None),
            info("child", Some(("parent", spawn(None)))),
        ];
        // A folder filter that keeps the child and not its parent.
        assert_eq!(shape(&arrange(&sessions, &[1])), [(1, 0)]);
        assert_eq!(shape(&arrange(&sessions, &[0, 1])), [(0, 0), (1, 1)]);
    }

    #[test]
    fn a_self_link_or_a_cycle_loses_no_session_and_never_loops() {
        let sessions = [
            info("a", Some(("b", ParentKind::fork()))),
            info("b", Some(("a", ParentKind::fork()))),
            info("me", Some(("me", ParentKind::fork()))),
        ];
        let mut indices: Vec<_> = all(&sessions).iter().map(|p| p.index).collect();
        indices.sort_unstable();
        assert_eq!(indices, [0, 1, 2]);
        assert!(all(&sessions).contains(&Place { index: 2, depth: 0 }));
    }

    #[test]
    fn a_repeated_id_is_arranged_once_each() {
        let sessions = [
            info("same", None),
            info("same", None),
            info("kid", Some(("same", spawn(None)))),
        ];
        assert_eq!(all(&sessions).len(), 3);
    }

    #[test]
    fn a_spawn_and_a_fork_are_marked_differently() {
        assert_eq!(
            mark(&info("c", Some(("p", spawn(Some("explore")))))).as_deref(),
            Some("↳ agent explore")
        );
        assert_eq!(
            mark(&info("c", Some(("p", spawn(None))))).as_deref(),
            Some("↳ agent")
        );
        assert_eq!(
            mark(&info("c", Some(("p", ParentKind::fork())))).as_deref(),
            Some("⑂ fork")
        );
        let mut unknown = info("c", None);
        unknown.parent = Some("p".to_string());
        assert_eq!(mark(&unknown).as_deref(), Some("↳"));
        assert_eq!(mark(&info("root", None)), None);
    }

    #[test]
    fn the_kind_text_names_the_agent_the_boundary_and_inherited_context() {
        let inherited = ParentKind::Spawn {
            call_id: None,
            agent: Some("reviewer".into()),
            inherits_context: true,
        };
        assert_eq!(
            kind_text(&info("c", Some(("p", inherited)))).as_deref(),
            Some("subagent · reviewer · started from the parent's conversation")
        );
        assert_eq!(
            kind_text(&info(
                "c",
                Some((
                    "p",
                    ParentKind::Fork {
                        at: Some("e7".into())
                    }
                ))
            ))
            .as_deref(),
            Some("fork · at e7")
        );
        assert_eq!(
            kind_text(&info("c", Some(("p", ParentKind::fork())))).as_deref(),
            Some("fork")
        );
        assert_eq!(kind_text(&info("root", None)), None);
    }

    #[test]
    fn children_are_counted_by_the_parent_link_and_told_apart_by_kind() {
        let mut linked = info("d", None);
        linked.parent = Some("p".to_string());
        let sessions = [
            info("p", None),
            info("a", Some(("p", spawn(None)))),
            info("b", Some(("p", ParentKind::fork()))),
            info("c", Some(("a", spawn(None)))),
            info("e", Some(("p", spawn(Some("explore"))))),
            linked,
        ];
        let of_p = children_of(&sessions, "p");
        assert_eq!(
            of_p,
            Children {
                subagents: 2,
                forks: 1,
                other: 1
            }
        );
        assert_eq!(of_p.total(), 4);
        assert_eq!(
            of_p.text().as_deref(),
            Some("4 (2 subagents, 1 fork, 1 linked)")
        );
        assert_eq!(
            children_of(&sessions, "a").text().as_deref(),
            Some("1 (1 subagent)")
        );
        assert_eq!(children_of(&sessions, "c").text(), None);
    }

    #[test]
    fn the_prefix_indents_two_cells_a_level_and_leads_with_the_mark() {
        let child = info("c", Some(("p", spawn(Some("explore")))));
        assert_eq!(prefix(&child, 0), "↳ agent explore ");
        assert_eq!(prefix(&child, 2), "    ↳ agent explore ");
        assert_eq!(prefix(&info("root", None), 0), "");
        assert_eq!(prefix(&info("root", None), 1), "  ");
    }
}
