use super::*;

const SDK_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SDK_WHEEL: &str = "soma_provider-0.2.0-cp311-abi3-manylinux_2_17_x86_64.whl";
const PLATFORM_TAG: &str = "manylinux_2_17_x86_64";

fn runtime() -> PythonRuntimeFingerprint {
    PythonRuntimeFingerprint::new("cpython", "3.12.4", "linux-x86_64", PLATFORM_TAG).unwrap()
}

fn wheel() -> &'static Path {
    Path::new(SDK_WHEEL)
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
        "# requires-python = \">= 3.11, < 4\"\r\n",
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
    assert_eq!(metadata.requires_python.as_deref(), Some(">=3.11, <4"));
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

    let invalid_requires = "# /// script\n# requires-python = \"not-a-version-range\"\n# ///\n";
    assert!(matches!(
        parse_pep723_metadata(invalid_requires),
        Err(PythonEnvironmentError::InvalidRequiresPython { .. })
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
    let first_plan = plan_python_environment(
        root,
        first.as_ref(),
        &runtime(),
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();
    let second_plan = plan_python_environment(
        root,
        second.as_ref(),
        &runtime(),
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();

    assert_eq!(first_plan, second_plan);
    assert_eq!(first_plan.key.len(), 64);
    assert_eq!(
        first_plan.directory,
        root.join("python").join("v2").join(&first_plan.key)
    );
    assert_eq!(first_plan.dependency_count, 2);
    assert_eq!(
        first_plan.sdk_wheel_tag,
        PythonWheelTag {
            python: "cp311".to_owned(),
            abi: "abi3".to_owned(),
            platform: PLATFORM_TAG.to_owned(),
        }
    );
}

#[test]
fn runtime_sdk_wheel_and_dependencies_are_cache_boundaries() {
    let root = Path::new("/cache");
    let base = Pep723Metadata {
        dependencies: vec!["httpx>=0.27".to_owned()],
        ..Pep723Metadata::default()
    };
    let baseline = plan_python_environment(
        root,
        Some(&base),
        &runtime(),
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();

    let mut changed_dependencies = base.clone();
    changed_dependencies
        .dependencies
        .push("anyio>=4".to_owned());
    let dependency_plan = plan_python_environment(
        root,
        Some(&changed_dependencies),
        &runtime(),
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();
    let runtime_plan = plan_python_environment(
        root,
        Some(&base),
        &PythonRuntimeFingerprint::new("cpython", "3.13.0", "linux-x86_64", PLATFORM_TAG).unwrap(),
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();
    let wheel_tag_plan = plan_python_environment(
        root,
        Some(&base),
        &runtime(),
        Path::new("soma_provider-0.2.0-cp310-abi3-manylinux_2_17_x86_64.whl"),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();
    let sdk_plan = plan_python_environment(
        root,
        Some(&base),
        &runtime(),
        wheel(),
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "0.11.31",
    )
    .unwrap();

    assert_ne!(baseline.key, dependency_plan.key);
    assert_ne!(baseline.key, runtime_plan.key);
    assert_ne!(baseline.key, wheel_tag_plan.key);
    assert_ne!(baseline.key, sdk_plan.key);
}

#[test]
fn validates_requires_python_against_selected_interpreter() {
    let compatible = Pep723Metadata {
        requires_python: Some(">=3.11, <3.13".to_owned()),
        ..Pep723Metadata::default()
    };
    plan_python_environment(
        Path::new("/cache"),
        Some(&compatible),
        &runtime(),
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();

    let incompatible = Pep723Metadata {
        requires_python: Some(">=3.13".to_owned()),
        ..Pep723Metadata::default()
    };
    assert_eq!(
        plan_python_environment(
            Path::new("/cache"),
            Some(&incompatible),
            &runtime(),
            wheel(),
            SDK_DIGEST,
            "0.11.31",
        )
        .unwrap_err(),
        PythonEnvironmentError::IncompatiblePython {
            version: "3.12.4".to_owned(),
            requires_python: ">=3.13".to_owned(),
        }
    );
}

#[test]
fn validates_abi3_baseline_implementation_and_platform_tag() {
    let latest =
        PythonRuntimeFingerprint::new("CPython", "3.14.0", "linux-x86_64", PLATFORM_TAG).unwrap();
    plan_python_environment(
        Path::new("/cache"),
        None,
        &latest,
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();

    let too_old =
        PythonRuntimeFingerprint::new("cpython", "3.10.14", "linux-x86_64", PLATFORM_TAG).unwrap();
    assert!(matches!(
        plan_python_environment(
            Path::new("/cache"),
            None,
            &too_old,
            wheel(),
            SDK_DIGEST,
            "0.11.31",
        ),
        Err(PythonEnvironmentError::IncompatibleSdkWheel { .. })
    ));

    let pypy =
        PythonRuntimeFingerprint::new("pypy", "3.12.0", "linux-x86_64", PLATFORM_TAG).unwrap();
    assert!(matches!(
        plan_python_environment(
            Path::new("/cache"),
            None,
            &pypy,
            wheel(),
            SDK_DIGEST,
            "0.11.31",
        ),
        Err(PythonEnvironmentError::IncompatibleSdkWheel { .. })
    ));

    let wrong_platform =
        PythonRuntimeFingerprint::new("cpython", "3.12.4", "windows-x86_64", "win_amd64").unwrap();
    assert!(matches!(
        plan_python_environment(
            Path::new("/cache"),
            None,
            &wrong_platform,
            wheel(),
            SDK_DIGEST,
            "0.11.31",
        ),
        Err(PythonEnvironmentError::IncompatibleSdkWheel { .. })
    ));
}

#[test]
fn accepts_matching_release_workflow_platform_tags() {
    let cases = [
        (
            "linux-x86_64",
            "manylinux_2_17_x86_64",
            "soma_provider-0.2.0-cp311-abi3-manylinux_2_17_x86_64.whl",
        ),
        (
            "windows-x86_64",
            "win_amd64",
            "soma_provider-0.2.0-cp311-abi3-win_amd64.whl",
        ),
        (
            "macos-x86_64",
            "macosx_10_12_x86_64",
            "soma_provider-0.2.0-cp311-abi3-macosx_10_12_x86_64.whl",
        ),
    ];

    for (platform, wheel_platform_tag, wheel) in cases {
        let runtime =
            PythonRuntimeFingerprint::new("cpython", "3.13.1", platform, wheel_platform_tag)
                .unwrap();
        let plan = plan_python_environment(
            Path::new("/cache"),
            None,
            &runtime,
            Path::new(wheel),
            SDK_DIGEST,
            "0.11.31",
        )
        .unwrap();

        assert_eq!(plan.sdk_wheel_tag.platform, wheel_platform_tag);
    }
}

#[test]
fn rejects_malformed_or_non_abi3_sdk_wheels() {
    assert!(matches!(
        plan_python_environment(
            Path::new("/cache"),
            None,
            &runtime(),
            Path::new("soma_provider.whl"),
            SDK_DIGEST,
            "0.11.31",
        ),
        Err(PythonEnvironmentError::InvalidSdkWheelFilename { .. })
    ));
    assert!(matches!(
        plan_python_environment(
            Path::new("/cache"),
            None,
            &runtime(),
            Path::new("soma_provider-0.2.0-py3-none-any.whl"),
            SDK_DIGEST,
            "0.11.31",
        ),
        Err(PythonEnvironmentError::IncompatibleSdkWheel { .. })
    ));
}

#[test]
fn planning_validates_immutable_inputs_and_has_no_side_effects() {
    let temporary = tempfile::tempdir().unwrap();
    let cache_root = temporary.path().join("missing-cache-root");
    let plan = plan_python_environment(
        &cache_root,
        None,
        &runtime(),
        wheel(),
        SDK_DIGEST,
        "0.11.31",
    )
    .unwrap();

    assert!(!cache_root.exists());
    assert_eq!(plan.dependency_count, 0);
    assert_eq!(
        plan_python_environment(&cache_root, None, &runtime(), wheel(), "bad", "0.11.31",)
            .unwrap_err(),
        PythonEnvironmentError::InvalidSdkDigest
    );
    assert!(matches!(
        PythonRuntimeFingerprint::new("", "3.12", "linux", PLATFORM_TAG),
        Err(PythonEnvironmentError::EmptyFingerprint {
            field: "implementation"
        })
    ));
    assert!(matches!(
        PythonRuntimeFingerprint::new("cpython", "three.twelve", "linux", PLATFORM_TAG),
        Err(PythonEnvironmentError::InvalidRuntimeVersion { .. })
    ));
    assert!(matches!(
        PythonRuntimeFingerprint::new("cpython", "3.12", "linux", "manylinux.2.17.x86_64",),
        Err(PythonEnvironmentError::InvalidFingerprintComponent {
            field: "wheel_platform_tag"
        })
    ));
}
