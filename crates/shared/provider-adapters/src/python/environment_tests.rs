use super::*;

const SDK_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn runtime() -> PythonRuntimeFingerprint {
    PythonRuntimeFingerprint::new("cpython", "3.12.4", "linux-x86_64").unwrap()
}

#[test]
fn source_without_metadata_requires_no_execution() {
    let source = "raise RuntimeError('must not execute')\n";
    assert_eq!(parse_pep723_metadata(source).unwrap(), None);
}

#[test]
fn parses_and_normalizes_pep723_metadata() {
    let source = concat!(
        "# /// script\r\n",
        "# requires-python = \">=3.11\"\r\n",
        "# dependencies = [\r\n",
        "#   \"httpx>=0.27\",\r\n",
        "#   \" anyio>=4 \",\r\n",
        "#   \"httpx>=0.27\",\r\n",
        "# ]\r\n",
        "# [tool.uv]\r\n",
        "# prerelease = \"disallow\"\r\n",
        "# ///\r\n",
        "raise RuntimeError('must not execute')\r\n",
    );

    let metadata = parse_pep723_metadata(source).unwrap().unwrap();
    assert_eq!(metadata.requires_python.as_deref(), Some(">=3.11"));
    assert_eq!(metadata.dependencies, ["anyio>=4", "httpx>=0.27"]);
    assert_eq!(
        metadata
            .uv
            .as_ref()
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("prerelease"))
            .and_then(toml::Value::as_str),
        Some("disallow")
    );
}

#[test]
fn rejects_structurally_invalid_blocks() {
    assert_eq!(
        parse_pep723_metadata("# /// script\n# dependencies = []\n").unwrap_err(),
        PythonEnvironmentError::UnterminatedScriptBlock
    );
    assert_eq!(
        parse_pep723_metadata("# /// script\ndependencies = []\n# ///\n").unwrap_err(),
        PythonEnvironmentError::NonCommentLine { line: 2 }
    );
    assert_eq!(
        parse_pep723_metadata(
            "# /// script\n# dependencies = []\n# ///\n# /// script\n# dependencies = []\n# ///\n"
        )
        .unwrap_err(),
        PythonEnvironmentError::MultipleScriptBlocks
    );
}

#[test]
fn rejects_invalid_metadata_values() {
    let empty_dependency = "# /// script\n# dependencies = [\"  \"]\n# ///\n";
    assert!(matches!(
        parse_pep723_metadata(empty_dependency),
        Err(PythonEnvironmentError::InvalidMetadata {
            field: "dependency",
            ..
        })
    ));

    let invalid_uv = "# /// script\n# [tool]\n# uv = \"not-a-table\"\n# ///\n";
    assert!(matches!(
        parse_pep723_metadata(invalid_uv),
        Err(PythonEnvironmentError::InvalidMetadata {
            field: "tool.uv",
            ..
        })
    ));
}

#[test]
fn equivalent_metadata_has_one_content_address() {
    let first = parse_pep723_metadata(
        "# /// script\n# dependencies = [\"httpx>=0.27\", \"anyio>=4\"]\n# ///\n",
    )
    .unwrap();
    let second = parse_pep723_metadata(
        "# /// script\n# dependencies = [\n#   \"anyio>=4\",\n#   \"httpx>=0.27\",\n# ]\n# ///\n",
    )
    .unwrap();
    let root = Path::new("/var/cache/soma");
    let first_plan =
        plan_python_environment(root, first.as_ref(), &runtime(), SDK_DIGEST, "0.11.31").unwrap();
    let second_plan =
        plan_python_environment(root, second.as_ref(), &runtime(), SDK_DIGEST, "0.11.31").unwrap();

    assert_eq!(first_plan, second_plan);
    assert_eq!(first_plan.key.len(), 64);
    assert_eq!(
        first_plan.directory,
        root.join("python").join("v1").join(&first_plan.key)
    );
    assert_eq!(first_plan.dependency_count, 2);
}

#[test]
fn runtime_sdk_and_dependencies_are_cache_boundaries() {
    let root = Path::new("/cache");
    let base = Pep723Metadata {
        dependencies: vec!["httpx>=0.27".to_owned()],
        ..Pep723Metadata::default()
    };
    let baseline =
        plan_python_environment(root, Some(&base), &runtime(), SDK_DIGEST, "0.11.31").unwrap();

    let mut changed_dependencies = base.clone();
    changed_dependencies
        .dependencies
        .push("anyio>=4".to_owned());
    let dependency_plan = plan_python_environment(
        root,
        Some(&changed_dependencies),
        &runtime(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();
    let runtime_plan = plan_python_environment(
        root,
        Some(&base),
        &PythonRuntimeFingerprint::new("cpython", "3.13.0", "linux-x86_64").unwrap(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();
    let sdk_plan = plan_python_environment(
        root,
        Some(&base),
        &runtime(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "0.11.31",
    )
    .unwrap();

    assert_ne!(baseline.key, dependency_plan.key);
    assert_ne!(baseline.key, runtime_plan.key);
    assert_ne!(baseline.key, sdk_plan.key);
}

#[test]
fn planning_validates_immutable_inputs_and_has_no_side_effects() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path().join("missing-cache-root");
    let plan =
        plan_python_environment(&cache_root, None, &runtime(), SDK_DIGEST, "0.11.31").unwrap();

    assert!(!cache_root.exists());
    assert_eq!(plan.dependency_count, 0);
    assert_eq!(
        plan_python_environment(&cache_root, None, &runtime(), "bad", "0.11.31").unwrap_err(),
        PythonEnvironmentError::InvalidSdkDigest
    );
    assert!(matches!(
        PythonRuntimeFingerprint::new("", "3.12", "linux"),
        Err(PythonEnvironmentError::EmptyFingerprint {
            field: "implementation"
        })
    ));
}
