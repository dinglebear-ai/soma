use std::fs;

use super::*;

fn runtime() -> PythonRuntimeFingerprint {
    PythonRuntimeFingerprint::new("cpython", "3.12.4", "linux-x86_64", "manylinux_2_17_x86_64")
        .unwrap()
}

fn wheel_tag() -> PythonWheelTag {
    PythonWheelTag {
        python: "cp311".to_owned(),
        abi: "abi3".to_owned(),
        platform: "manylinux_2_17_x86_64".to_owned(),
    }
}

fn write_ready_entry(cache_root: &Path, version: u32, key: &str) -> PathBuf {
    let directory = cache_root
        .join("python")
        .join(format!("v{version}"))
        .join(key);
    let python = directory.join(".venv/bin/python");
    fs::create_dir_all(python.parent().unwrap()).unwrap();
    fs::write(&python, "python").unwrap();
    let lock = b"version = 1";
    fs::write(directory.join("uv.lock"), lock).unwrap();
    let marker = ReadyMarker {
        schema_version: READY_SCHEMA_VERSION,
        environment_key: key.to_owned(),
        plan_version: version,
        dependency_count: 2,
        runtime: runtime(),
        sdk_wheel_tag: wheel_tag(),
        sdk_wheel_sha256: "a".repeat(64),
        uv_version: "0.11.31".to_owned(),
        lock_sha256: sha256_hex(lock),
    };
    fs::write(
        directory.join(READY_FILE),
        serde_json::to_vec_pretty(&marker).unwrap(),
    )
    .unwrap();
    directory
}

#[test]
fn missing_cache_root_has_empty_inventory() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path().join("missing");
    let cache = PythonEnvironmentCache::new(&cache_root);

    let inventory = cache.inventory().unwrap();

    assert_eq!(inventory.root, cache_root.join("python"));
    assert!(inventory.entries.is_empty());
    assert_eq!(inventory.summary, PythonEnvironmentCacheSummary::default());
}

#[test]
fn inventories_ready_incomplete_invalid_and_staging_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path();
    let ready = write_ready_entry(cache_root, 2, "b-ready");
    let version = cache_root.join("python/v2");

    let incomplete = version.join("a-incomplete");
    fs::create_dir_all(&incomplete).unwrap();
    fs::write(incomplete.join("partial"), "partial").unwrap();

    let invalid = version.join("c-invalid");
    fs::create_dir_all(&invalid).unwrap();
    fs::write(invalid.join(READY_FILE), "not-json").unwrap();

    let staging = version.join(".d-ready.tmp-123-0");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("partial"), "partial").unwrap();

    fs::write(version.join("e-file"), "not a directory").unwrap();

    let inventory = PythonEnvironmentCache::new(cache_root).inventory().unwrap();

    assert_eq!(inventory.entries.len(), 5);
    assert_eq!(inventory.summary.ready, 1);
    assert_eq!(inventory.summary.incomplete, 1);
    assert_eq!(inventory.summary.invalid, 2);
    assert_eq!(inventory.summary.staging, 1);
    assert!(inventory.summary.total_size_bytes > 0);
    assert!(inventory.summary.total_file_count >= 7);
    assert!(inventory
        .entries
        .windows(2)
        .all(|pair| pair[0].directory <= pair[1].directory));

    let ready_entry = inventory
        .entries
        .iter()
        .find(|entry| entry.directory == ready)
        .unwrap();
    assert_eq!(ready_entry.state, PythonEnvironmentCacheState::Ready);
    assert_eq!(ready_entry.key.as_deref(), Some("b-ready"));
    assert_eq!(ready_entry.plan_directory_version, Some(2));
    assert!(ready_entry.issue.is_none());
    let metadata = ready_entry.metadata.as_ref().unwrap();
    assert_eq!(metadata.environment_key, "b-ready");
    assert_eq!(metadata.plan_version, 2);
    assert_eq!(metadata.dependency_count, 2);
    assert_eq!(metadata.runtime, runtime());
    assert_eq!(metadata.sdk_wheel_tag, wheel_tag());

    let incomplete_entry = inventory
        .entries
        .iter()
        .find(|entry| entry.directory == incomplete)
        .unwrap();
    assert_eq!(
        incomplete_entry.state,
        PythonEnvironmentCacheState::Incomplete
    );
    assert!(incomplete_entry
        .issue
        .as_deref()
        .unwrap()
        .contains("readiness marker"));

    let invalid_entry = inventory
        .entries
        .iter()
        .find(|entry| entry.directory == invalid)
        .unwrap();
    assert_eq!(invalid_entry.state, PythonEnvironmentCacheState::Invalid);
    assert!(invalid_entry
        .issue
        .as_deref()
        .unwrap()
        .contains("marker is invalid"));

    let staging_entry = inventory
        .entries
        .iter()
        .find(|entry| entry.directory == staging)
        .unwrap();
    assert_eq!(staging_entry.state, PythonEnvironmentCacheState::Staging);
    assert!(staging_entry.key.is_none());
}

#[test]
fn detects_lock_marker_and_version_directory_mismatches() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path();
    let tampered = write_ready_entry(cache_root, 2, "tampered");
    fs::write(tampered.join("uv.lock"), "tampered lock").unwrap();

    let wrong_key = write_ready_entry(cache_root, 2, "wrong-key");
    let marker_path = wrong_key.join(READY_FILE);
    let mut marker: ReadyMarker = serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    marker.environment_key = "different-key".to_owned();
    fs::write(&marker_path, serde_json::to_vec_pretty(&marker).unwrap()).unwrap();

    let wrong_version = write_ready_entry(cache_root, 3, "wrong-version");
    let marker_path = wrong_version.join(READY_FILE);
    let mut marker: ReadyMarker = serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    marker.plan_version = 2;
    fs::write(&marker_path, serde_json::to_vec_pretty(&marker).unwrap()).unwrap();

    let inventory = PythonEnvironmentCache::new(cache_root).inventory().unwrap();

    assert_eq!(inventory.summary.invalid, 3);
    assert_eq!(inventory.summary.ready, 0);
    let issues = inventory
        .entries
        .iter()
        .map(|entry| entry.issue.as_deref().unwrap())
        .collect::<Vec<_>>();
    assert!(issues.iter().any(|issue| issue.contains("uv.lock digest")));
    assert!(issues.iter().any(|issue| issue.contains("directory name")));
    assert!(issues
        .iter()
        .any(|issue| issue.contains("version directory")));
}

#[cfg(unix)]
#[test]
fn rejects_symlink_roots_and_never_follows_symlink_entries() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let target_root = temporary.path().join("target");
    fs::create_dir_all(target_root.join("python")).unwrap();
    let linked_cache_root = temporary.path().join("linked-cache");
    fs::create_dir_all(&linked_cache_root).unwrap();
    symlink(target_root.join("python"), linked_cache_root.join("python")).unwrap();

    assert!(matches!(
        PythonEnvironmentCache::new(&linked_cache_root).inventory(),
        Err(PythonEnvironmentCacheError::UnsafeRoot { .. })
    ));

    let cache_root = temporary.path().join("real-cache");
    let version = cache_root.join("python/v2");
    fs::create_dir_all(&version).unwrap();
    let outside = temporary.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret"), "do not count this file").unwrap();
    let linked_entry = version.join("linked-entry");
    symlink(&outside, &linked_entry).unwrap();

    let inventory = PythonEnvironmentCache::new(&cache_root)
        .inventory()
        .unwrap();

    assert_eq!(inventory.entries.len(), 1);
    assert_eq!(inventory.summary.invalid, 1);
    assert_eq!(inventory.entries[0].directory, linked_entry);
    assert_eq!(inventory.entries[0].file_count, 0);
    assert!(inventory.entries[0]
        .issue
        .as_deref()
        .unwrap()
        .contains("symbolic link"));
}

#[test]
fn prune_plan_is_a_dry_run_and_never_selects_ready_entries() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path();
    let ready = write_ready_entry(cache_root, 2, "ready");
    let version = cache_root.join("python/v2");
    let incomplete = version.join("incomplete");
    fs::create_dir_all(&incomplete).unwrap();
    let invalid = version.join("invalid");
    fs::create_dir_all(&invalid).unwrap();
    fs::write(invalid.join(READY_FILE), "not-json").unwrap();
    let staging = version.join(".candidate.tmp-1-0");
    fs::create_dir_all(&staging).unwrap();

    let cache = PythonEnvironmentCache::new(cache_root);
    let plan = cache
        .plan_prune(PythonEnvironmentPrunePolicy::conservative(u64::MAX))
        .unwrap();

    assert_eq!(plan.candidates.len(), 3);
    assert!(plan
        .candidates
        .iter()
        .all(|candidate| candidate.entry.state != PythonEnvironmentCacheState::Ready));
    assert!(plan.reclaimable_size_bytes > 0);
    assert!(ready.exists());
    assert!(incomplete.exists());
    assert!(invalid.exists());
    assert!(staging.exists());
}

#[test]
fn prune_plan_respects_the_stale_cutoff() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path();
    let incomplete = cache_root.join("python/v2/incomplete");
    fs::create_dir_all(&incomplete).unwrap();
    let cache = PythonEnvironmentCache::new(cache_root);

    let plan = cache
        .plan_prune(PythonEnvironmentPrunePolicy::conservative(0))
        .unwrap();

    assert!(plan.candidates.is_empty());
    assert!(incomplete.exists());
}

#[test]
fn prune_apply_removes_only_unchanged_candidates() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path();
    let ready = write_ready_entry(cache_root, 2, "ready");
    let version = cache_root.join("python/v2");
    let incomplete = version.join("incomplete");
    fs::create_dir_all(&incomplete).unwrap();
    fs::write(incomplete.join("partial"), "partial").unwrap();
    let invalid = version.join("invalid");
    fs::create_dir_all(&invalid).unwrap();
    fs::write(invalid.join(READY_FILE), "not-json").unwrap();
    let staging = version.join(".candidate.tmp-1-0");
    fs::create_dir_all(&staging).unwrap();

    let cache = PythonEnvironmentCache::new(cache_root);
    let plan = cache
        .plan_prune(PythonEnvironmentPrunePolicy::conservative(u64::MAX))
        .unwrap();
    fs::write(invalid.join("changed"), "changed after planning").unwrap();
    fs::remove_dir_all(&staging).unwrap();

    let report = cache.apply_prune(&plan).unwrap();

    assert_eq!(report.removed, 1);
    assert_eq!(report.changed, 1);
    assert_eq!(report.missing, 1);
    assert!(report.reclaimed_size_bytes > 0);
    assert!(!incomplete.exists());
    assert!(invalid.exists());
    assert!(!staging.exists());
    assert!(ready.exists());
}

#[test]
fn prune_rejects_ready_candidates_and_foreign_roots() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path();
    write_ready_entry(cache_root, 2, "ready");
    let cache = PythonEnvironmentCache::new(cache_root);
    let ready = cache
        .inventory()
        .unwrap()
        .entries
        .into_iter()
        .find(|entry| entry.state == PythonEnvironmentCacheState::Ready)
        .unwrap();
    let policy = PythonEnvironmentPrunePolicy::conservative(u64::MAX);
    let ready_plan = PythonEnvironmentPrunePlan {
        root: cache.root().to_path_buf(),
        policy,
        reclaimable_size_bytes: ready.size_bytes,
        reclaimable_file_count: ready.file_count,
        candidates: vec![PythonEnvironmentPruneCandidate {
            entry: ready,
            reason: "malicious ready candidate".to_owned(),
        }],
    };
    assert!(matches!(
        cache.apply_prune(&ready_plan),
        Err(PythonEnvironmentPruneError::ReadyEnvironment { .. })
    ));

    let foreign_plan = PythonEnvironmentPrunePlan {
        root: temporary.path().join("foreign"),
        policy,
        candidates: Vec::new(),
        reclaimable_size_bytes: 0,
        reclaimable_file_count: 0,
    };
    assert!(matches!(
        cache.apply_prune(&foreign_plan),
        Err(PythonEnvironmentPruneError::RootMismatch { .. })
    ));
}

#[cfg(unix)]
#[test]
fn prune_removes_symlink_entry_without_touching_target() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path().join("cache");
    let version = cache_root.join("python/v2");
    fs::create_dir_all(&version).unwrap();
    let outside = temporary.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let secret = outside.join("secret");
    fs::write(&secret, "preserve me").unwrap();
    let linked_entry = version.join("linked-entry");
    symlink(&outside, &linked_entry).unwrap();
    let cache = PythonEnvironmentCache::new(&cache_root);
    let plan = cache
        .plan_prune(PythonEnvironmentPrunePolicy::conservative(u64::MAX))
        .unwrap();

    let report = cache.apply_prune(&plan).unwrap();

    assert_eq!(report.removed, 1);
    assert!(!linked_entry.exists());
    assert!(secret.is_file());
    assert_eq!(fs::read_to_string(secret).unwrap(), "preserve me");
}
