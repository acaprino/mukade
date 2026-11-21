//! The work on disk: what git knows about a session's folder, read-only.
//!
//! A session has a working folder; this module says which repositories it holds and what in them
//! exists nowhere else (commits on no remote, uncommitted paths). It runs the installed `git` and
//! only reads: no fetch, no command that writes, and `GIT_OPTIONAL_LOCKS=0` so `git status` does
//! not even refresh the index. It names no harness.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// How long one git command may take.
const GIT_TIMEOUT: Duration = Duration::from_secs(10);

/// The git state of one folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Work {
    pub folder: PathBuf,
    pub repositories: Vec<Repository>,
    /// Why there is nothing to show: the folder is missing, git is not installed, no repository.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Repository {
    /// The folder as it was given, not resolved.
    pub path: PathBuf,
    /// The main worktree first, then the linked ones.
    pub worktrees: Vec<Worktree>,
    pub branches: Vec<Branch>,
    /// What git answered when it refused this repository; the lists are then empty.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Worktree {
    /// As git prints it (resolved).
    pub path: PathBuf,
    /// `None`: a detached head.
    pub branch: Option<String>,
    /// The short commit id; empty before the first commit.
    pub head: String,
    /// Tracked changes and untracked files; ignored files are not counted.
    pub uncommitted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Branch {
    pub name: String,
    pub upstream: Option<String>,
    /// Commits ahead of the upstream; `None` without one.
    pub ahead: Option<usize>,
    /// Commits reachable from the branch and from no remote-tracking ref.
    pub unpushed: usize,
}

impl Repository {
    /// Whether work here exists nowhere else: uncommitted paths, or a branch with unpushed commits.
    pub fn at_risk(&self) -> bool {
        self.worktrees
            .iter()
            .any(|worktree| worktree.uncommitted > 0)
            || self.branches.iter().any(|branch| branch.unpushed > 0)
    }
}

/// The repositories of `folder`: itself when it is one, else each direct child that is, by name.
pub fn work_in(folder: &Path) -> Work {
    if !folder.exists() {
        return nothing(folder, format!("{} does not exist", folder.display()));
    }
    if !folder.is_dir() {
        return nothing(folder, format!("{} is not a folder", folder.display()));
    }
    let paths = repository_paths(folder);
    if paths.is_empty() {
        return nothing(folder, format!("no git repository in {}", folder.display()));
    }
    let mut repositories = Vec::new();
    for path in paths {
        match inspect(&path) {
            Ok(repository) => repositories.push(repository),
            Err(GitError::NotInstalled) => return nothing(folder, "git is not installed".into()),
            Err(GitError::Refused(error)) => repositories.push(Repository {
                path,
                worktrees: Vec::new(),
                branches: Vec::new(),
                error: Some(error),
            }),
        }
    }
    Work {
        folder: folder.to_path_buf(),
        repositories,
        note: None,
    }
}

fn nothing(folder: &Path, note: String) -> Work {
    Work {
        folder: folder.to_path_buf(),
        repositories: Vec::new(),
        note: Some(note),
    }
}

/// Whether `path` holds a `.git`, a folder or (in a linked worktree or a submodule) a file.
fn holds_git(path: &Path) -> bool {
    path.join(".git").exists()
}

fn repository_paths(folder: &Path) -> Vec<PathBuf> {
    if holds_git(folder) {
        return vec![folder.to_path_buf()];
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && holds_git(path))
        .collect();
    // The children share a parent, so this is the order of their names.
    found.sort();
    found
}

// ---------------------------------------------------------------------------------------------
// git

enum GitError {
    NotInstalled,
    /// What git (or the wait for it) said when a command failed.
    Refused(String),
}

/// Run `git -C <dir> <args>` and return what it printed. Nothing here writes to a repository.
fn git(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    let finished = crate::exe::run_within(&mut command, GIT_TIMEOUT).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            GitError::NotInstalled
        } else {
            GitError::Refused(format!("git could not start: {error}"))
        }
    })?;
    let Some(status) = finished.status else {
        return Err(GitError::Refused(format!(
            "git did not answer within {} s",
            GIT_TIMEOUT.as_secs()
        )));
    };
    if !status.success() {
        let said = String::from_utf8_lossy(&finished.stderr);
        let said = said.trim();
        return Err(GitError::Refused(if said.is_empty() {
            format!("git exited with {status}")
        } else {
            said.to_string()
        }));
    }
    Ok(String::from_utf8_lossy(&finished.stdout).into_owned())
}

/// What `git worktree list --porcelain` says of one worktree.
struct Listed {
    path: PathBuf,
    branch: Option<String>,
    head: String,
    bare: bool,
    prunable: bool,
}

fn parse_worktrees(text: &str) -> Vec<Listed> {
    let mut found: Vec<Listed> = Vec::new();
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            found.push(Listed {
                path: PathBuf::from(path),
                branch: None,
                head: String::new(),
                bare: false,
                prunable: false,
            });
        } else if let Some(current) = found.last_mut() {
            if let Some(id) = line.strip_prefix("HEAD ") {
                current.head = short_id(id);
            } else if let Some(name) = line.strip_prefix("branch ") {
                current.branch = Some(name.strip_prefix("refs/heads/").unwrap_or(name).to_string());
            } else if line == "bare" {
                current.bare = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                current.prunable = true;
            }
        }
    }
    found
}

/// The first seven characters of a commit id; nothing for the null id of a repository with no
/// commit yet.
fn short_id(id: &str) -> String {
    if id.chars().all(|c| c == '0') {
        return String::new();
    }
    id.chars().take(7).collect()
}

/// Entries of `git status`: tracked changes and untracked files, one per path.
fn uncommitted(worktree: &Path) -> Result<usize, GitError> {
    let out = git(
        worktree,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ],
    )?;
    Ok(out.split('\0').filter(|entry| !entry.is_empty()).count())
}

fn rev_count(path: &Path, args: &[&str]) -> Result<usize, GitError> {
    let mut all: Vec<&str> = vec!["rev-list", "--count"];
    all.extend_from_slice(args);
    let out = git(path, &all)?;
    out.trim()
        .parse()
        .map_err(|_| GitError::Refused(format!("git answered `{}` to a commit count", out.trim())))
}

fn inspect(path: &Path) -> Result<Repository, GitError> {
    let listing = git(path, &["worktree", "list", "--porcelain"])?;
    let mut worktrees = Vec::new();
    for listed in parse_worktrees(&listing) {
        // A bare repository has no files; a prunable worktree's folder is gone.
        if listed.bare || listed.prunable {
            continue;
        }
        let uncommitted = uncommitted(&listed.path)?;
        worktrees.push(Worktree {
            path: listed.path,
            branch: listed.branch,
            head: listed.head,
            uncommitted,
        });
    }
    let refs = git(
        path,
        &[
            "for-each-ref",
            "--format=%(refname:short)%09%(upstream:short)",
            "refs/heads",
        ],
    )?;
    let mut branches = Vec::new();
    for line in refs.lines() {
        let (name, upstream) = line.split_once('\t').unwrap_or((line, ""));
        if name.is_empty() {
            continue;
        }
        let upstream = (!upstream.is_empty()).then(|| upstream.to_string());
        // An upstream that is gone has no count: that is not an error of the repository.
        let ahead = upstream.as_deref().and_then(|upstream| {
            let range = format!("{upstream}..refs/heads/{name}");
            rev_count(path, &[range.as_str(), "--"]).ok()
        });
        let branch_ref = format!("refs/heads/{name}");
        let unpushed = rev_count(path, &[branch_ref.as_str(), "--not", "--remotes", "--"])?;
        branches.push(Branch {
            name: name.to_string(),
            upstream,
            ahead,
            unpushed,
        });
    }
    Ok(Repository {
        path: path.to_path_buf(),
        worktrees,
        branches,
        error: None,
    })
}

// ---------------------------------------------------------------------------------------------
// the lines

/// One line per repository and per branch or worktree that has something to say, for a terminal
/// and for the handoff note.
pub fn lines(work: &Work) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(note) = &work.note {
        out.push(note.clone());
    }
    for repository in &work.repositories {
        repository_lines(repository, &mut out);
    }
    if work
        .repositories
        .iter()
        .any(|repository| repository.error.is_none())
    {
        out.push("(remote state as of the last fetch)".to_string());
    }
    out
}

fn repository_lines(repository: &Repository, out: &mut Vec<String>) {
    let name = repository
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| repository.path.display().to_string());
    if let Some(error) = &repository.error {
        out.push(format!("{name}: {error}"));
        return;
    }
    // The main worktree is the first one git lists.
    let main = repository.worktrees.first();
    let main_branch = main.and_then(|worktree| worktree.branch.as_deref());
    match main {
        None => out.push(format!("{name}: no worktree")),
        Some(worktree) => {
            let text = match &worktree.branch {
                Some(branch) => {
                    let known = repository
                        .branches
                        .iter()
                        .find(|known| known.name == *branch)
                        .cloned()
                        // A branch with no commit yet is in no list of refs.
                        .unwrap_or_else(|| Branch {
                            name: branch.clone(),
                            upstream: None,
                            ahead: None,
                            unpushed: 0,
                        });
                    branch_text(&known, worktree.uncommitted)
                }
                None => detached_text(&worktree.head, worktree.uncommitted),
            };
            out.push(format!("{name}: {text}"));
        }
    }
    let linked = repository.worktrees.iter().skip(1);
    for branch in &repository.branches {
        if Some(branch.name.as_str()) == main_branch {
            continue;
        }
        let uncommitted = linked
            .clone()
            .find(|worktree| worktree.branch.as_deref() == Some(branch.name.as_str()))
            .map_or(0, |worktree| worktree.uncommitted);
        if branch.unpushed > 0 || branch.ahead.is_some_and(|ahead| ahead > 0) || uncommitted > 0 {
            out.push(format!("  {}", branch_text(branch, uncommitted)));
        }
    }
    for worktree in linked {
        if worktree.branch.is_none() && worktree.uncommitted > 0 {
            out.push(format!(
                "  {}",
                detached_text(&worktree.head, worktree.uncommitted)
            ));
        }
    }
}

fn branch_text(branch: &Branch, uncommitted: usize) -> String {
    let mut parts = vec![format!("branch {}", branch.name)];
    parts.push(match &branch.upstream {
        Some(upstream) => format!("upstream {upstream}"),
        None => "no upstream".to_string(),
    });
    let mut say = Vec::new();
    match branch.ahead {
        Some(0) if uncommitted > 0 => say.push("in sync".to_string()),
        Some(ahead) if ahead > 0 => say.push(format!("{} ahead", plural(ahead, "commit"))),
        _ => {}
    }
    if branch.unpushed > 0 && branch.ahead != Some(branch.unpushed) {
        say.push(format!(
            "{} on no remote",
            plural(branch.unpushed, "commit")
        ));
    }
    if uncommitted > 0 {
        say.push(plural(uncommitted, "uncommitted path"));
    }
    if say.is_empty() {
        say.push("clean".to_string());
    }
    parts.extend(say);
    parts.join(", ")
}

fn detached_text(head: &str, uncommitted: usize) -> String {
    let at = if head.is_empty() {
        "detached head".to_string()
    } else {
        format!("detached at {head}")
    };
    if uncommitted > 0 {
        format!("{at}, {}", plural(uncommitted, "uncommitted path"))
    } else {
        format!("{at}, clean")
    }
}

fn plural(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::tests::scratch;
    use crate::scratch::Scratch;

    fn skip() -> bool {
        let missing = !Command::new("git")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if missing {
            eprintln!("git is not installed: test skipped");
        }
        missing
    }

    /// Run git in `dir` with a fixed identity, and fail the test when git fails.
    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "init.defaultBranch=main",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Write `file` and commit it.
    fn commit(dir: &Path, file: &str, content: &str) {
        std::fs::write(dir.join(file), content).unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", file]);
    }

    /// A scratch folder holding `origin` (one commit on `main`) and `clone` of it; returns the
    /// folder and the clone.
    fn clone_of_origin(tag: &str) -> (Scratch, PathBuf) {
        let dir = scratch(tag);
        let origin = dir.join("origin");
        std::fs::create_dir(&origin).unwrap();
        git(&origin, &["init", "-q"]);
        commit(&origin, "a.txt", "one\n");
        git(&dir, &["clone", "-q", "origin", "clone"]);
        let clone = dir.join("clone");
        (dir, clone)
    }

    fn same_folder(a: &Path, b: &Path) -> bool {
        std::fs::canonicalize(a).unwrap() == std::fs::canonicalize(b).unwrap()
    }

    fn only(work: &Work) -> &Repository {
        assert_eq!(work.note, None);
        assert_eq!(work.repositories.len(), 1, "{work:?}");
        &work.repositories[0]
    }

    fn branch<'a>(repository: &'a Repository, name: &str) -> &'a Branch {
        repository
            .branches
            .iter()
            .find(|branch| branch.name == name)
            .unwrap_or_else(|| panic!("no branch {name} in {repository:?}"))
    }

    #[test]
    fn a_clean_clone_in_sync_has_nothing_at_risk() {
        if skip() {
            return;
        }
        let (_dir, clone) = clone_of_origin("work-clean");
        let work = work_in(&clone);
        let repository = only(&work);
        assert_eq!(repository.error, None);
        assert_eq!(repository.branches.len(), 1);
        let main = branch(repository, "main");
        assert_eq!(main.upstream.as_deref(), Some("origin/main"));
        assert_eq!(main.ahead, Some(0));
        assert_eq!(main.unpushed, 0);
        assert_eq!(repository.worktrees.len(), 1);
        assert_eq!(repository.worktrees[0].uncommitted, 0);
        assert!(!repository.at_risk());
    }

    #[test]
    fn uncommitted_paths_count_tracked_changes_and_untracked_files() {
        if skip() {
            return;
        }
        let (_dir, clone) = clone_of_origin("work-uncommitted");
        commit(&clone, ".gitignore", "*.log\n");
        std::fs::write(clone.join("a.txt"), "changed\n").unwrap();
        std::fs::write(clone.join("new1.txt"), "x").unwrap();
        std::fs::write(clone.join("new2.txt"), "x").unwrap();
        std::fs::write(clone.join("ignored.log"), "x").unwrap();
        let work = work_in(&clone);
        let repository = only(&work);
        assert_eq!(repository.worktrees[0].uncommitted, 3);
        assert!(repository.at_risk());
    }

    #[test]
    fn a_branch_with_no_upstream_counts_its_commits_on_no_remote() {
        if skip() {
            return;
        }
        let (_dir, clone) = clone_of_origin("work-no-upstream");
        git(&clone, &["checkout", "-q", "-b", "topic"]);
        commit(&clone, "b.txt", "two\n");
        commit(&clone, "c.txt", "three\n");
        let work = work_in(&clone);
        let repository = only(&work);
        let topic = branch(repository, "topic");
        assert_eq!(topic.upstream, None);
        assert_eq!(topic.ahead, None);
        assert_eq!(topic.unpushed, 2);
        assert_eq!(branch(repository, "main").unpushed, 0);
        assert!(repository.at_risk());
    }

    #[test]
    fn commits_ahead_of_the_upstream_are_counted() {
        if skip() {
            return;
        }
        let (_dir, clone) = clone_of_origin("work-ahead");
        commit(&clone, "b.txt", "two\n");
        let work = work_in(&clone);
        let repository = only(&work);
        let main = branch(repository, "main");
        assert_eq!(main.ahead, Some(1));
        assert_eq!(main.unpushed, 1);
        assert!(repository.at_risk());
    }

    #[test]
    fn a_linked_worktree_is_listed_with_its_own_branch_and_changes() {
        if skip() {
            return;
        }
        let (dir, clone) = clone_of_origin("work-linked");
        let linked = dir.join("wt");
        git(&clone, &["worktree", "add", "-q", "../wt", "-b", "side/x"]);
        std::fs::write(linked.join("new.txt"), "x").unwrap();
        let work = work_in(&clone);
        let repository = only(&work);
        assert_eq!(repository.worktrees.len(), 2, "{repository:?}");
        let main = &repository.worktrees[0];
        assert!(same_folder(&main.path, &clone), "{:?}", main.path);
        assert_eq!(main.branch.as_deref(), Some("main"));
        assert_eq!(main.uncommitted, 0);
        let side = &repository.worktrees[1];
        assert!(same_folder(&side.path, &linked), "{:?}", side.path);
        assert_eq!(side.branch.as_deref(), Some("side/x"));
        assert_eq!(side.uncommitted, 1);
        // A branch name with a slash is read whole.
        assert_eq!(branch(repository, "side/x").unpushed, 0);
    }

    #[test]
    fn a_detached_head_has_no_branch() {
        if skip() {
            return;
        }
        let (_dir, clone) = clone_of_origin("work-detached");
        git(&clone, &["checkout", "-q", "--detach"]);
        let work = work_in(&clone);
        let repository = only(&work);
        let head = &repository.worktrees[0];
        assert_eq!(head.branch, None);
        assert!(head.head.len() >= 7, "{:?}", head.head);
        assert!(head.head.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn a_folder_that_is_a_repository_is_the_only_one() {
        if skip() {
            return;
        }
        let (_dir, clone) = clone_of_origin("work-itself");
        // Its sibling `origin` is a repository too: it is not looked at.
        let work = work_in(&clone);
        assert_eq!(only(&work).path, clone);
        assert_eq!(work.folder, clone);
    }

    #[test]
    fn the_repositories_one_level_down_are_found_by_name() {
        if skip() {
            return;
        }
        let dir = scratch("work-children");
        for name in ["b", "a", "c"] {
            std::fs::create_dir(dir.join(name)).unwrap();
        }
        git(&dir.join("a"), &["init", "-q"]);
        git(&dir.join("b"), &["init", "-q"]);
        let work = work_in(&dir);
        assert_eq!(work.note, None);
        let names: Vec<_> = work
            .repositories
            .iter()
            .map(|repository| repository.path.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(names, ["a", "b"]);
        // A repository with no commit yet reads without an error.
        let empty = &work.repositories[0];
        assert_eq!(empty.error, None);
        assert!(empty.branches.is_empty());
        assert_eq!(empty.worktrees[0].branch.as_deref(), Some("main"));
        assert_eq!(empty.worktrees[0].head, "");
        assert!(!empty.at_risk());
    }

    #[test]
    fn work_in_a_missing_folder_says_so() {
        let dir = scratch("work-missing");
        let work = work_in(&dir.join("nope"));
        assert!(work.repositories.is_empty());
        let note = work.note.expect("a note");
        assert!(note.contains("does not exist"), "{note}");
    }

    #[test]
    fn a_folder_with_no_repository_says_so() {
        let dir = scratch("work-none");
        std::fs::create_dir(dir.join("plain")).unwrap();
        let work = work_in(&dir);
        assert!(work.repositories.is_empty());
        let note = work.note.expect("a note");
        assert!(note.contains("no git repository"), "{note}");
    }

    #[test]
    fn a_repository_git_refuses_keeps_its_error_and_the_others() {
        if skip() {
            return;
        }
        let dir = scratch("work-refused");
        std::fs::create_dir(dir.join("x")).unwrap();
        std::fs::write(dir.join("x").join(".git"), "").unwrap();
        std::fs::create_dir(dir.join("y")).unwrap();
        git(&dir.join("y"), &["init", "-q"]);
        let work = work_in(&dir);
        assert_eq!(work.note, None);
        assert_eq!(work.repositories.len(), 2, "{work:?}");
        let x = &work.repositories[0];
        assert!(x.error.is_some(), "{x:?}");
        assert!(x.worktrees.is_empty() && x.branches.is_empty());
        let y = &work.repositories[1];
        assert_eq!(y.error, None);
        assert_eq!(y.worktrees.len(), 1);
        let said = lines(&work);
        assert!(said[0].starts_with("x: "), "{said:?}");
        assert!(said[1].starts_with("y: "), "{said:?}");
    }

    #[test]
    fn reading_changes_nothing() {
        if skip() {
            return;
        }
        let (_dir, clone) = clone_of_origin("work-readonly");
        // Rewrite a tracked file after a pause: its stat data no longer matches the index, which
        // is what a plain `git status` would refresh and write back.
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(clone.join("a.txt"), "one\n").unwrap();
        let index = clone.join(".git").join("index");
        let before = (
            std::fs::read(&index).unwrap(),
            std::fs::metadata(&index).unwrap().modified().unwrap(),
        );
        let work = work_in(&clone);
        assert_eq!(only(&work).error, None);
        let after = (
            std::fs::read(&index).unwrap(),
            std::fs::metadata(&index).unwrap().modified().unwrap(),
        );
        assert!(before == after, "the index was rewritten");
    }

    fn worktree(branch: Option<&str>, uncommitted: usize) -> Worktree {
        Worktree {
            path: PathBuf::from("/work/tree"),
            branch: branch.map(str::to_string),
            head: "abc1234".to_string(),
            uncommitted,
        }
    }

    fn tracking(
        name: &str,
        upstream: Option<&str>,
        ahead: Option<usize>,
        unpushed: usize,
    ) -> Branch {
        Branch {
            name: name.to_string(),
            upstream: upstream.map(str::to_string),
            ahead,
            unpushed,
        }
    }

    fn repository(name: &str, worktrees: Vec<Worktree>, branches: Vec<Branch>) -> Repository {
        Repository {
            path: PathBuf::from("/work").join(name),
            worktrees,
            branches,
            error: None,
        }
    }

    fn work_of(repositories: Vec<Repository>) -> Work {
        Work {
            folder: PathBuf::from("/work"),
            repositories,
            note: None,
        }
    }

    #[test]
    fn the_lines_say_what_the_note_needs() {
        let work = work_of(vec![
            repository(
                "daodan",
                vec![worktree(Some("develop"), 1)],
                vec![tracking("develop", Some("origin/develop"), Some(0), 0)],
            ),
            repository(
                "stack-analyser",
                vec![worktree(Some("feature/stack-detection"), 133)],
                vec![tracking("feature/stack-detection", None, None, 3)],
            ),
            repository(
                "imago",
                vec![worktree(Some("develop"), 0)],
                vec![tracking("develop", Some("origin/develop"), Some(0), 0)],
            ),
        ]);
        assert_eq!(
            lines(&work),
            [
                "daodan: branch develop, upstream origin/develop, in sync, 1 uncommitted path",
                "stack-analyser: branch feature/stack-detection, no upstream, 3 commits on no remote, 133 uncommitted paths",
                "imago: branch develop, upstream origin/develop, clean",
                "(remote state as of the last fetch)",
            ]
        );
    }

    #[test]
    fn the_lines_count_commits_ahead_and_name_the_other_branches() {
        let mut main = repository(
            "app",
            vec![worktree(Some("main"), 0), worktree(Some("side"), 2)],
            vec![
                tracking("main", Some("origin/main"), Some(1), 1),
                tracking("quiet", Some("origin/quiet"), Some(0), 0),
                tracking("side", None, None, 1),
            ],
        );
        main.worktrees.push(worktree(None, 4));
        let work = work_of(vec![main]);
        assert_eq!(
            lines(&work),
            [
                "app: branch main, upstream origin/main, 1 commit ahead",
                "  branch side, no upstream, 1 commit on no remote, 2 uncommitted paths",
                "  detached at abc1234, 4 uncommitted paths",
                "(remote state as of the last fetch)",
            ]
        );
        let two = repository(
            "two",
            vec![worktree(Some("main"), 0)],
            vec![tracking("main", Some("origin/main"), Some(2), 2)],
        );
        assert_eq!(
            lines(&work_of(vec![two])),
            [
                "two: branch main, upstream origin/main, 2 commits ahead",
                "(remote state as of the last fetch)",
            ]
        );
    }

    #[test]
    fn the_lines_report_an_error_and_a_note() {
        let refused = Repository {
            path: PathBuf::from("/work/x"),
            worktrees: Vec::new(),
            branches: Vec::new(),
            error: Some("fatal: nope".to_string()),
        };
        assert_eq!(lines(&work_of(vec![refused])), ["x: fatal: nope"]);
        let note = Work {
            folder: PathBuf::from("/work"),
            repositories: Vec::new(),
            note: Some("/work does not exist".to_string()),
        };
        assert_eq!(lines(&note), ["/work does not exist"]);
    }
}
