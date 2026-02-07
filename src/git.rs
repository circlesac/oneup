use anyhow::{Context, Result, bail};
use git2::{Repository, Signature};
use std::path::Path;

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    /// Open the git repository containing the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let repo =
            Repository::discover(path).context("not a git repository (or any parent up to /)")?;
        Ok(Self { repo })
    }

    /// Check if the working tree is clean.
    pub fn is_clean(&self) -> Result<bool> {
        let statuses = self
            .repo
            .statuses(Some(
                git2::StatusOptions::new()
                    .include_untracked(true)
                    .recurse_untracked_dirs(true),
            ))
            .context("failed to get git status")?;

        Ok(statuses.is_empty())
    }

    /// Stage files, commit, and create an annotated tag.
    pub fn commit_and_tag(
        &self,
        file_paths: &[&Path],
        version: &str,
        message: &str,
    ) -> Result<()> {
        let msg = message.replace("%s", version);
        let tag_name = format!("v{version}");

        // Check if tag already exists
        if self.repo.revparse_single(&tag_name).is_ok() {
            bail!("tag {tag_name} already exists (use --force to overwrite)");
        }

        let (tree, sig, parent) = self.stage_and_prepare(file_paths, &msg)?;

        // Create commit
        let commit_oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, &msg, &tree, &[&parent])
            .context("failed to create commit")?;

        // Create annotated tag
        let commit_obj = self.repo.find_object(commit_oid, None)?;
        self.repo
            .tag(&tag_name, &commit_obj, &sig, &msg, false)
            .with_context(|| format!("failed to create tag {tag_name}"))?;

        Ok(())
    }

    /// Force-create a tag (overwrite if exists), used with --force.
    pub fn commit_and_tag_force(
        &self,
        file_paths: &[&Path],
        version: &str,
        message: &str,
    ) -> Result<()> {
        let msg = message.replace("%s", version);
        let tag_name = format!("v{version}");

        // Delete existing tag if present
        if self.repo.revparse_single(&tag_name).is_ok() {
            let _ = self.repo.tag_delete(&tag_name);
        }

        let (tree, sig, parent) = self.stage_and_prepare(file_paths, &msg)?;

        let commit_oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, &msg, &tree, &[&parent])
            .context("failed to create commit")?;

        let commit_obj = self.repo.find_object(commit_oid, None)?;
        self.repo
            .tag(&tag_name, &commit_obj, &sig, &msg, true)
            .with_context(|| format!("failed to create tag {tag_name}"))?;

        Ok(())
    }

    fn stage_and_prepare(
        &self,
        file_paths: &[&Path],
        _msg: &str,
    ) -> Result<(git2::Tree<'_>, Signature<'_>, git2::Commit<'_>)> {
        let mut index = self.repo.index().context("failed to open index")?;
        let workdir = self
            .repo
            .workdir()
            .context("bare repositories are not supported")?;
        let workdir_canon = workdir.canonicalize()?;

        for file_path in file_paths {
            let relative = file_path
                .canonicalize()?
                .strip_prefix(&workdir_canon)
                .context("target file is not inside the repository")?
                .to_path_buf();
            index
                .add_path(&relative)
                .with_context(|| format!("failed to stage {}", relative.display()))?;
        }

        index.write().context("failed to write index")?;
        let tree_oid = index.write_tree().context("failed to write tree")?;
        let tree = self.repo.find_tree(tree_oid)?;

        let sig = self
            .repo
            .signature()
            .or_else(|_| Signature::now("bump", "bump@noreply"))
            .context("failed to determine git signature")?;

        let parent = self.repo.head()?.peel_to_commit()?;

        Ok((tree, sig, parent))
    }
}
