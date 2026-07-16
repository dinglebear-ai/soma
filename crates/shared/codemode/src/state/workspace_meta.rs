use regex::Regex;
use serde::Serialize;

use crate::path_safety::{reject_existing_symlink_ancestors, rel_to_unix_string};
use crate::ToolError;

use super::path::{is_reserved_metadata_path, VirtualPath};
use super::workspace::{
    internal_io, not_found_or_internal, serialize_error, ExistsResult, GlobResult, ListResult,
    MutationResult, ReplaceInFilesResult, SearchFilesResult, SearchMatch, StatResult,
    StateWorkspace, WalkEntry, WalkTreeResult,
};

impl StateWorkspace {
    pub async fn exists(&self, path: &VirtualPath) -> Result<ExistsResult, ToolError> {
        let destination = self.resolve(path);
        self.ensure_path_allowed(&destination).await?;
        let exists = match tokio::fs::metadata(&destination).await {
            Ok(_) => true,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
            Err(err) => return Err(internal_io("read state path metadata")(err)),
        };
        Ok(ExistsResult {
            path: path.as_str().to_string(),
            exists,
        })
    }

    pub async fn stat(&self, path: &VirtualPath) -> Result<StatResult, ToolError> {
        let destination = self.resolve(path);
        self.ensure_path_allowed(&destination).await?;
        let metadata = tokio::fs::metadata(&destination)
            .await
            .map_err(not_found_or_internal("read state path metadata"))?;
        let kind = if metadata.is_file() {
            "file"
        } else if metadata.is_dir() {
            "directory"
        } else {
            return Err(ToolError::Sdk {
                sdk_kind: "permission_denied".to_string(),
                message: "state path kind is not supported".to_string(),
            });
        };
        Ok(StatResult {
            path: path.as_str().to_string(),
            kind: kind.to_string(),
            bytes: metadata.len(),
        })
    }

    pub async fn mkdir(&self, path: &VirtualPath) -> Result<MutationResult, ToolError> {
        let destination = self.resolve(path);
        self.ensure_path_allowed(&destination).await?;
        self.check_entry_quota_for_path(&destination).await?;
        tokio::fs::create_dir_all(&destination)
            .await
            .map_err(internal_io("create state directory"))?;
        self.ensure_path_allowed(&destination).await?;
        Ok(MutationResult {
            ok: true,
            path: path.as_str().to_string(),
        })
    }

    pub async fn remove(
        &self,
        path: &VirtualPath,
        recursive: bool,
    ) -> Result<MutationResult, ToolError> {
        if is_reserved_metadata_path(path.as_str()) {
            return Err(ToolError::Sdk {
                sdk_kind: "permission_denied".to_string(),
                message: "state metadata paths cannot be removed".to_string(),
            });
        }
        let destination = self.resolve(path);
        self.ensure_path_allowed(&destination).await?;
        let metadata = tokio::fs::metadata(&destination)
            .await
            .map_err(not_found_or_internal("read state path metadata"))?;
        if metadata.is_file() {
            tokio::fs::remove_file(&destination)
                .await
                .map_err(internal_io("remove state file"))?;
        } else if metadata.is_dir() {
            if recursive {
                tokio::fs::remove_dir_all(&destination)
                    .await
                    .map_err(internal_io("remove state directory tree"))?;
            } else {
                tokio::fs::remove_dir(&destination)
                    .await
                    .map_err(internal_io("remove state directory"))?;
            }
        } else {
            return Err(ToolError::Sdk {
                sdk_kind: "permission_denied".to_string(),
                message: "state path kind is not supported".to_string(),
            });
        }
        Ok(MutationResult {
            ok: true,
            path: path.as_str().to_string(),
        })
    }

    pub async fn copy(
        &self,
        from: &VirtualPath,
        to: &VirtualPath,
    ) -> Result<MutationResult, ToolError> {
        let source = self.read_file(from).await?;
        self.write_file(to, &source.content).await?;
        Ok(MutationResult {
            ok: true,
            path: to.as_str().to_string(),
        })
    }

    pub async fn move_path(
        &self,
        from: &VirtualPath,
        to: &VirtualPath,
    ) -> Result<MutationResult, ToolError> {
        let source = self.resolve(from);
        let destination = self.resolve(to);
        self.ensure_path_allowed(&source).await?;
        self.ensure_path_allowed(&destination).await?;
        self.check_entry_quota_for_path(&destination).await?;
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(internal_io("create state move directory"))?;
        }
        reject_existing_symlink_ancestors(self.root(), &destination)?;
        tokio::fs::rename(&source, &destination)
            .await
            .map_err(not_found_or_internal("move state path"))?;
        Ok(MutationResult {
            ok: true,
            path: to.as_str().to_string(),
        })
    }

    pub async fn walk_tree(
        &self,
        path: &VirtualPath,
        limit: usize,
    ) -> Result<WalkTreeResult, ToolError> {
        let limit = normalize_limit(limit);
        let start = self.resolve(path);
        self.ensure_path_allowed(&start).await?;
        let mut entries = Vec::new();
        let mut stack = vec![start];
        while let Some(dir) = stack.pop() {
            let mut read_dir = tokio::fs::read_dir(&dir)
                .await
                .map_err(not_found_or_internal("read state directory"))?;
            while let Some(entry) = read_dir
                .next_entry()
                .await
                .map_err(internal_io("read state directory entry"))?
            {
                let path = entry.path();
                let relative = match path.strip_prefix(self.root()) {
                    Ok(relative) => relative,
                    Err(_) => continue,
                };
                let virtual_path = rel_to_unix_string(relative);
                if is_reserved_metadata_path(&virtual_path) {
                    continue;
                }
                let metadata = tokio::fs::symlink_metadata(&path)
                    .await
                    .map_err(internal_io("read state workspace metadata"))?;
                if metadata.file_type().is_symlink() {
                    return Err(ToolError::Sdk {
                        sdk_kind: "symlink_rejected".to_string(),
                        message: "state walk rejected a symlink".to_string(),
                    });
                }
                let kind = if metadata.is_dir() {
                    stack.push(path);
                    "directory"
                } else if metadata.is_file() {
                    "file"
                } else {
                    return Err(ToolError::Sdk {
                        sdk_kind: "permission_denied".to_string(),
                        message: "state path kind is not supported".to_string(),
                    });
                };
                entries.push(WalkEntry {
                    path: virtual_path,
                    kind: kind.to_string(),
                    bytes: metadata.len(),
                });
                if entries.len() > limit {
                    entries.sort_by(|left, right| left.path.cmp(&right.path));
                    entries.truncate(limit);
                    return Ok(WalkTreeResult {
                        entries,
                        truncated: true,
                    });
                }
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(WalkTreeResult {
            entries,
            truncated: false,
        })
    }

    pub async fn list(&self, path: &VirtualPath) -> Result<ListResult, ToolError> {
        let dir = self.resolve(path);
        self.ensure_path_allowed(&dir).await?;
        let mut read_dir = tokio::fs::read_dir(&dir)
            .await
            .map_err(not_found_or_internal("read state directory"))?;
        let mut entries = Vec::new();
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(internal_io("read state directory entry"))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let child_path = if path.as_str().is_empty() {
                name.clone()
            } else {
                format!("{}/{}", path.as_str(), name)
            };
            if is_reserved_metadata_path(&child_path) {
                continue;
            }
            entries.push(name);
            if entries.len() as u64 > self.limits().max_entries {
                return Err(ToolError::Sdk {
                    sdk_kind: "response_too_large".to_string(),
                    message: "state list exceeded max entries".to_string(),
                });
            }
        }
        entries.sort();
        Ok(ListResult { entries })
    }

    pub async fn glob(&self, pattern: &str, limit: usize) -> Result<GlobResult, ToolError> {
        let limit = normalize_limit(limit);
        let matcher = glob_pattern_regex(pattern)?;
        let walked = self.walk_files(self.limits().max_entries as usize).await?;
        let mut matches = Vec::new();
        for file in walked.files {
            if matcher.is_match(&file) {
                matches.push(file);
                if matches.len() > limit {
                    matches.truncate(limit);
                    return Ok(GlobResult {
                        matches,
                        truncated: true,
                    });
                }
            }
        }
        Ok(GlobResult {
            matches,
            truncated: walked.truncated,
        })
    }

    pub async fn search_files(
        &self,
        pattern: &str,
        query: &str,
        limit: usize,
    ) -> Result<SearchFilesResult, ToolError> {
        if query.is_empty() {
            return Err(ToolError::InvalidParam {
                message: "state search query must not be empty".to_string(),
                param: "query".to_string(),
            });
        }
        let limit = normalize_limit(limit);
        let glob = self
            .glob(pattern, self.limits().max_entries as usize)
            .await?;
        let mut matches = Vec::new();
        for path in glob.matches {
            let virtual_path = VirtualPath::parse(&path)?;
            let file = self.read_file(&virtual_path).await?;
            for (index, line) in file.content.lines().enumerate() {
                if line.contains(query) {
                    matches.push(SearchMatch {
                        path: path.clone(),
                        line: index + 1,
                        text: cap_line_preview(line),
                    });
                    if matches.len() > limit {
                        matches.truncate(limit);
                        return Ok(SearchFilesResult {
                            matches,
                            truncated: true,
                        });
                    }
                    ensure_serialized_result_fits(&matches, self.limits().max_result_bytes)?;
                }
            }
        }
        Ok(SearchFilesResult {
            matches,
            truncated: glob.truncated,
        })
    }

    pub async fn replace_in_files(
        &self,
        pattern: &str,
        search: &str,
        replace: &str,
        dry_run: bool,
    ) -> Result<ReplaceInFilesResult, ToolError> {
        if search.is_empty() {
            return Err(ToolError::InvalidParam {
                message: "state replace search must not be empty".to_string(),
                param: "search".to_string(),
            });
        }
        let changed_paths = self.replace_targets(pattern, search).await?;
        let changed = changed_paths
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>();
        if dry_run {
            return Ok(ReplaceInFilesResult { changed, dry_run });
        }
        let mut originals = Vec::new();
        for path in changed_paths {
            let file = self.read_file(&path).await?;
            if !file.content.contains(search) {
                return Err(self
                    .restore_originals_after_failure(
                        &originals,
                        ToolError::Sdk {
                            sdk_kind: "edit_conflict".to_string(),
                            message: format!(
                                "state replace input no longer matches `{}`",
                                path.as_str()
                            ),
                        },
                    )
                    .await);
            }
            let original = file.content;
            let next = original.replace(search, replace);
            if let Err(err) = self.write_file(&path, &next).await {
                return Err(self.restore_originals_after_failure(&originals, err).await);
            }
            originals.push((path, original));
        }
        Ok(ReplaceInFilesResult { changed, dry_run })
    }

    async fn replace_targets(
        &self,
        pattern: &str,
        search: &str,
    ) -> Result<Vec<VirtualPath>, ToolError> {
        let glob = self
            .glob(pattern, self.limits().max_entries as usize)
            .await?;
        if glob.truncated {
            return Err(ToolError::Sdk {
                sdk_kind: "response_too_large".to_string(),
                message: "state replace input exceeded max entries".to_string(),
            });
        }
        let mut changed_paths = Vec::new();
        for path in glob.matches {
            let virtual_path = VirtualPath::parse(&path)?;
            let file = self.read_file(&virtual_path).await?;
            if file.content.contains(search) {
                changed_paths.push(virtual_path);
            }
        }
        Ok(changed_paths)
    }
}

fn normalize_limit(limit: usize) -> usize {
    limit.clamp(1, 10_000)
}

fn glob_pattern_regex(pattern: &str) -> Result<Regex, ToolError> {
    if pattern.trim().is_empty() {
        return Err(ToolError::InvalidParam {
            message: "state glob pattern must not be empty".to_string(),
            param: "pattern".to_string(),
        });
    }
    if pattern.contains("..") || pattern.starts_with('/') || pattern.contains(':') {
        return Err(ToolError::Sdk {
            sdk_kind: "path_traversal".to_string(),
            message: "state glob pattern must stay inside the workspace".to_string(),
        });
    }
    let mut regex = String::from("^");
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') && chars.get(i + 2) == Some(&'/') => {
                regex.push_str("(?:.*/)?");
                i += 3;
            }
            '*' if chars.get(i + 1) == Some(&'*') => {
                regex.push_str(".*");
                i += 2;
            }
            '*' => {
                regex.push_str("[^/]*");
                i += 1;
            }
            '?' => {
                regex.push_str("[^/]");
                i += 1;
            }
            ch => {
                regex.push_str(&regex::escape(&ch.to_string()));
                i += 1;
            }
        }
    }
    regex.push('$');
    Regex::new(&regex).map_err(|err| ToolError::InvalidParam {
        message: format!("invalid state glob pattern: {err}"),
        param: "pattern".to_string(),
    })
}

fn cap_line_preview(line: &str) -> String {
    line.chars().take(512).collect()
}

fn ensure_serialized_result_fits<T: Serialize>(value: &T, max: usize) -> Result<(), ToolError> {
    let len = serde_json::to_vec(value).map_err(serialize_error)?.len();
    if len > max {
        return Err(ToolError::Sdk {
            sdk_kind: "response_too_large".to_string(),
            message: "state search result exceeded max result bytes".to_string(),
        });
    }
    Ok(())
}
