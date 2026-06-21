//! The staging index: a persistent partial `Manifest` (`<repo>/index`) that
//! `commit` turns into the next tree. A *missing* index file means "index ≡
//! HEAD's tree" — a clean index is the file's absence, never a copy of HEAD.

use crate::manifest::Manifest;
use crate::repository::Repository;
use crate::status::Change;
use crate::{pathspec, snapshot, RepoError, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn index_path(repo: &Repository) -> PathBuf {
    repo.dir().join("index")
}

/// The staged tree, or `None` when there is no index file (clean).
pub fn read(repo: &Repository) -> Result<Option<Manifest>> {
    let p = index_path(repo);
    if !p.is_file() {
        return Ok(None);
    }
    Ok(Some(Manifest::from_json(&std::fs::read_to_string(p)?)?))
}

/// Write the staged tree atomically (temp + rename).
pub fn write(repo: &Repository, m: &Manifest) -> Result<()> {
    let p = index_path(repo);
    let tmp = p.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, m.to_json()?.as_bytes())?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// Remove the index file (→ clean: index ≡ HEAD).
pub fn clear(repo: &Repository) -> Result<()> {
    let _ = std::fs::remove_file(index_path(repo));
    Ok(())
}

/// HEAD's tree as a manifest, or an empty manifest when HEAD is unborn.
pub fn head_tree(repo: &Repository) -> Result<Manifest> {
    match repo.head_commit() {
        Some(h) => repo.read_manifest(&repo.read_commit(&h)?.tree),
        None => Ok(Manifest::default()),
    }
}

/// The effective staged tree: the index if present, else HEAD's tree, else an
/// empty manifest.
pub fn effective(repo: &Repository) -> Result<Manifest> {
    match read(repo)? {
        Some(m) => Ok(m),
        None => head_tree(repo),
    }
}

/// Stage the worktree state of every path selected by `specs` (relative to the
/// worktree root) into the index: update/insert entries for present files and
/// remove entries for in-scope paths that no longer exist (staged deletions).
/// Returns one [`Change`] per index entry this call touched (Added/Modified/
/// Removed relative to the index before the call), sorted by path. Errors if the
/// pathspecs match nothing (no worktree file and no in-scope index entry).
///
/// `progress`, when set, is called with `(files done, total in-scope files)` as
/// the worktree is scanned — a sign of life when many chunks must be decoded.
pub fn add_paths(
    repo: &Repository,
    world_dir: &Path,
    specs: &[String],
    progress: Option<snapshot::Progress>,
) -> Result<Vec<Change>> {
    let accept = |rel: &str| pathspec::matches_any(specs, rel);
    let partial = match progress {
        Some(p) => snapshot::snapshot_scoped_with_progress(repo, world_dir, &accept, p)?,
        None => snapshot::snapshot_scoped(repo, world_dir, &accept)?,
    };

    // Paths actually present in the worktree within scope.
    let present: HashSet<String> = partial
        .regions
        .keys()
        .chain(partial.nbt.keys())
        .chain(partial.blobs.keys())
        .cloned()
        .collect();

    let before = effective(repo)?;
    let mut idx = before.clone();

    // Overlay freshly-snapshotted in-scope entries.
    for (k, v) in partial.regions {
        idx.regions.insert(k, v);
    }
    for (k, v) in partial.nbt {
        idx.nbt.insert(k, v);
    }
    for (k, v) in partial.blobs {
        idx.blobs.insert(k, v);
    }

    // Staged deletions: in-scope index entries no longer in the worktree.
    idx.regions.retain(|k, _| !accept(k) || present.contains(k));
    idx.nbt.retain(|k, _| !accept(k) || present.contains(k));
    idx.blobs.retain(|k, _| !accept(k) || present.contains(k));

    // Recompute in-scope empty dirs.
    idx.empty_dirs.retain(|dir| !accept(dir));
    idx.empty_dirs.extend(partial.empty_dirs);
    idx.empty_dirs.sort();
    idx.empty_dirs.dedup();

    // Pathspec matched nothing at all → git-style error.
    let in_scope_before = before
        .regions
        .keys()
        .chain(before.nbt.keys())
        .chain(before.blobs.keys())
        .chain(before.empty_dirs.iter())
        .any(|k| accept(k));
    if present.is_empty() && !in_scope_before {
        return Err(RepoError::Other(format!(
            "pathspec '{}' did not match any files",
            specs.join(" ")
        )));
    }

    // What this call changed in the index, classified A/M/D vs. the prior index.
    let changes = crate::status::diff(&before, &idx);
    if idx == head_tree(repo)? {
        clear(repo)?; // result ≡ HEAD → clean index is the file's absence
    } else if !changes.is_empty() {
        write(repo, &idx)?;
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn repo() -> (tempfile::TempDir, Repository) {
        let d = tempfile::tempdir().unwrap();
        let repo = Repository::init(&d.path().join("repo")).unwrap();
        (d, repo)
    }

    #[test]
    fn absent_index_reads_none_and_effective_falls_back_to_head() {
        let (_d, repo) = repo();
        assert!(read(&repo).unwrap().is_none());
        // unborn HEAD → effective is the empty manifest
        assert_eq!(effective(&repo).unwrap(), Manifest::default());

        // commit an empty tree so HEAD exists
        let tree = repo.write_manifest(&Manifest::default()).unwrap();
        let c = repo.create_commit(&tree, vec![], "x", "me", "t").unwrap();
        repo.write_branch("main", &c).unwrap();
        // still no index file → effective == HEAD's tree
        assert!(read(&repo).unwrap().is_none());
        assert_eq!(effective(&repo).unwrap(), head_tree(&repo).unwrap());
    }

    #[test]
    fn write_read_clear_roundtrip() {
        let (_d, repo) = repo();
        let mut m = Manifest::default();
        m.blobs.insert("a.bin".into(), "deadbeef".into());

        write(&repo, &m).unwrap();
        assert_eq!(read(&repo).unwrap(), Some(m.clone()));
        assert_eq!(effective(&repo).unwrap(), m);

        clear(&repo).unwrap();
        assert!(read(&repo).unwrap().is_none());
        // clearing an already-absent index is a no-op (no error)
        clear(&repo).unwrap();
    }

    fn world(dir: &TempDir) -> std::path::PathBuf {
        let w = dir.path().join("world");
        std::fs::create_dir_all(w.join("sub")).unwrap();
        std::fs::write(w.join("a.bin"), b"alpha").unwrap();
        std::fs::write(w.join("sub").join("b.bin"), b"beta").unwrap();
        std::fs::write(w.join("c.bin"), b"gamma").unwrap();
        w
    }

    #[test]
    fn add_stages_a_single_file() {
        let (d, repo) = repo();
        let w = world(&d);
        let changes = add_paths(&repo, &w, &["a.bin".to_string()], None).unwrap();
        assert_eq!(changes.len(), 1);
        let idx = read(&repo).unwrap().unwrap();
        assert!(idx.blobs.contains_key("a.bin"));
        assert!(!idx.blobs.contains_key("c.bin"), "c.bin not staged");
        assert!(!idx.blobs.contains_key("sub/b.bin"), "sub/b.bin not staged");
    }

    #[test]
    fn add_directory_is_recursive() {
        let (d, repo) = repo();
        let w = world(&d);
        add_paths(&repo, &w, &["sub".to_string()], None).unwrap();
        let idx = read(&repo).unwrap().unwrap();
        assert!(idx.blobs.contains_key("sub/b.bin"));
        assert!(!idx.blobs.contains_key("a.bin"));
    }

    #[test]
    fn add_dot_stages_everything() {
        let (d, repo) = repo();
        let w = world(&d);
        let changes = add_paths(&repo, &w, &[".".to_string()], None).unwrap();
        assert_eq!(changes.len(), 3);
        let idx = read(&repo).unwrap().unwrap();
        assert_eq!(idx.blobs.len(), 3);
    }

    #[test]
    fn add_stages_a_deletion_within_scope() {
        let (d, repo) = repo();
        let w = world(&d);
        // stage everything, then delete a file and re-add its directory scope
        add_paths(&repo, &w, &[".".to_string()], None).unwrap();
        std::fs::remove_file(w.join("a.bin")).unwrap();
        add_paths(&repo, &w, &["a.bin".to_string()], None).unwrap();
        let idx = read(&repo).unwrap().unwrap();
        assert!(!idx.blobs.contains_key("a.bin"), "deletion staged");
        assert!(idx.blobs.contains_key("c.bin"), "others untouched");
    }

    #[test]
    fn add_nonmatching_pathspec_errors() {
        let (d, repo) = repo();
        let w = world(&d);
        let err = add_paths(&repo, &w, &["nope/x.bin".to_string()], None).unwrap_err();
        assert!(err.to_string().contains("did not match"), "got: {err}");
    }

    #[test]
    fn add_unchanged_file_returns_zero_changes() {
        let (d, repo) = repo();
        let w = world(&d);
        assert_eq!(
            add_paths(&repo, &w, &["a.bin".to_string()], None)
                .unwrap()
                .len(),
            1
        );
        // staging the identical file again changes nothing
        let changes = add_paths(&repo, &w, &["a.bin".to_string()], None).unwrap();
        assert!(changes.is_empty());
        assert!(read(&repo).unwrap().unwrap().blobs.contains_key("a.bin"));
    }

    #[test]
    fn add_reverting_to_head_clears_the_index() {
        let (d, repo) = repo();
        let world = d.path().join("world");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("a.bin"), b"v1").unwrap();
        // commit v1 as HEAD
        let m = snapshot::snapshot(&repo, &world).unwrap();
        let tree = repo.write_manifest(&m).unwrap();
        let c = repo.create_commit(&tree, vec![], "x", "me", "t").unwrap();
        repo.write_branch("main", &c).unwrap();
        // stage v2, then revert the worktree to v1 and re-add
        std::fs::write(world.join("a.bin"), b"v2").unwrap();
        add_paths(&repo, &world, &["a.bin".to_string()], None).unwrap();
        assert!(read(&repo).unwrap().is_some(), "v2 staged");
        std::fs::write(world.join("a.bin"), b"v1").unwrap();
        add_paths(&repo, &world, &["a.bin".to_string()], None).unwrap();
        assert!(
            read(&repo).unwrap().is_none(),
            "re-adding HEAD-equal content clears the index"
        );
    }

    #[test]
    fn add_stages_an_empty_dir_deletion() {
        let (d, repo) = repo();
        let w = world(&d);
        std::fs::create_dir(w.join("emptydir")).unwrap();
        add_paths(&repo, &w, &[".".to_string()], None).unwrap();
        assert!(read(&repo)
            .unwrap()
            .unwrap()
            .empty_dirs
            .contains(&"emptydir".to_string()));

        // remove the empty dir and re-stage just its scope
        std::fs::remove_dir(w.join("emptydir")).unwrap();
        let changes = add_paths(&repo, &w, &["emptydir".to_string()], None).unwrap();
        assert!(
            !changes.is_empty(),
            "empty-dir removal must be staged (not silently dropped)"
        );
        assert!(!read(&repo)
            .unwrap()
            .unwrap()
            .empty_dirs
            .contains(&"emptydir".to_string()));
    }

    #[test]
    fn add_reports_change_kinds_against_head() {
        use crate::status::ChangeKind;
        let (d, repo) = repo();
        let w = world(&d); // a.bin, c.bin, sub/b.bin
                           // commit the world as HEAD so add classifies against it
        let m = snapshot::snapshot(&repo, &w).unwrap();
        let tree = repo.write_manifest(&m).unwrap();
        let c = repo.create_commit(&tree, vec![], "x", "me", "t").unwrap();
        repo.write_branch("main", &c).unwrap();

        // modify a.bin, delete c.bin, add a brand-new d.bin
        std::fs::write(w.join("a.bin"), b"alpha2").unwrap();
        std::fs::remove_file(w.join("c.bin")).unwrap();
        std::fs::write(w.join("d.bin"), b"delta").unwrap();

        let changes = add_paths(&repo, &w, &[".".to_string()], None).unwrap();
        let kind = |p: &str| changes.iter().find(|c| c.path == p).map(|c| c.kind);
        assert_eq!(kind("a.bin"), Some(ChangeKind::Modified));
        assert_eq!(kind("c.bin"), Some(ChangeKind::Removed));
        assert_eq!(kind("d.bin"), Some(ChangeKind::Added));
        // sub/b.bin was untouched → not reported
        assert_eq!(kind("sub/b.bin"), None);
    }
}
