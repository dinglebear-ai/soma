fn normalize_workflow_newlines(workflow: &str) -> String {
    workflow.replace("\r\n", "\n").replace('\r', "\n")
}

fn workflow_job_block(workflow: &str, job_name: &str) -> String {
    let workflow = normalize_workflow_newlines(workflow);
    let marker = format!("  {job_name}:");
    let start = workflow
        .lines()
        .scan(0, |offset, line| {
            let line_start = *offset;
            *offset += line.len() + 1;
            Some((line_start, line))
        })
        .find_map(|(offset, line)| (line == marker).then_some(offset))
        .unwrap_or_else(|| panic!("missing workflow job {job_name}"));
    let rest = &workflow[start + marker.len()..];
    let end = rest
        .lines()
        .scan(0, |offset, line| {
            let line_start = *offset;
            *offset += line.len() + 1;
            Some((line_start, line))
        })
        .skip(1)
        .find_map(|(offset, line)| {
            if line.starts_with("  ") && !line.starts_with("    ") {
                Some(offset)
            } else {
                None
            }
        })
        .unwrap_or(rest.len());
    rest[..end].to_owned()
}

#[test]
fn ci_runs_release_version_gate_before_merge() {
    let workflow = include_str!("../../../.github/workflows/ci.yml");
    let soma = workflow_job_block(workflow, "soma");
    assert!(
        soma.contains("cargo xtask check-version-sync"),
        "CI must ensure version-bearing files stay internally synchronized"
    );
    assert!(
        soma.contains("fetch-depth: 0"),
        "version sync gate needs enough history for adjacent Soma checks"
    );
}

#[test]
fn native_builds_are_release_only_and_github_hosted() {
    let ci = include_str!("../../../.github/workflows/ci.yml");
    let release = include_str!("../../../.github/workflows/release.yml");
    assert!(
        !ci.contains("build-windows:") && !ci.contains("build-linux:"),
        "native artifact builds must not consume PR or main-branch CI capacity"
    );
    assert!(
        release.contains("release:\n    types: [published]")
            && !release.contains("workflow_dispatch:")
            && !release.contains("self-hosted"),
        "heavy native builds must run only for published releases on GitHub-hosted runners"
    );
}

#[test]
fn release_please_uses_ci_gated_release_pr_flow() {
    let workflow = include_str!("../../../.github/workflows/release-please.yml");
    let release_please = workflow_job_block(workflow, "release-please");
    let fixups = workflow_job_block(workflow, "release-pr-fixups");
    assert!(
        workflow.contains(r#"workflows: ["CI"]"#),
        "release-please must run only after CI succeeds on main"
    );
    assert!(
        release_please.contains("RELEASE_PLEASE_TOKEN"),
        "release-please must use a PAT/App token so downstream workflows fire"
    );
    assert!(
        release_please
            .contains("googleapis/release-please-action@45996ed1f6d02564a971a2fa1b5860e934307cf7"),
        "release-please action should be pinned to the documented SHA"
    );
    assert!(
        fixups.contains("cargo xtask sync-release-please-version"),
        "release PRs must sync derived version files after release-please updates the manifest"
    );
    assert!(
        fixups.contains("cargo xtask check-version-sync"),
        "release PR fixups must verify all version-bearing files agree"
    );
}

#[test]
fn artifact_workflows_run_from_published_releases() {
    let release =
        normalize_workflow_newlines(include_str!("../../../.github/workflows/release.yml"));
    let docker = normalize_workflow_newlines(include_str!(
        "../../../.github/workflows/docker-publish.yml"
    ));
    for workflow in [&release, &docker] {
        assert!(
            workflow.contains("release:\n    types: [published]"),
            "artifact workflow must trigger from release-please published releases"
        );
        assert!(
            !workflow.contains("workflow_dispatch:"),
            "heavy artifact workflows must run only from a published release"
        );
    }
    assert!(
        release.contains("validate-release-tag:") && docker.contains("validate:"),
        "artifact publication must validate the immutable release contract"
    );
    assert!(
        release.contains("gh release upload")
            && release.contains("\"${{ needs.validate-release-tag.outputs.release_tag }}\""),
        "release artifact workflow must attach files to the existing release tag"
    );
    assert!(
        !release.contains("git push origin HEAD:main") && !release.contains("ref: main"),
        "release artifact workflow must not write generated binaries back to main"
    );
    let npm = workflow_job_block(&release, "npm");
    assert!(
        npm.contains("needs: [validate-release-tag, release]")
            && npm.contains("npm-trusted-publish.yml@542ea7b7e5ca2d4e21f3277bfcf158584fee90ec"),
        "npm publish must wait for artifacts and use the fleet source of truth"
    );
    assert!(
        release.contains("arch: linux-x86_64")
            && release.contains("artifacts/${{ env.BINARY_NAME }}-${{ matrix.arch }}.tar.gz",),
        "release assets must include the installer's linux-x86_64 naming convention"
    );
    assert!(
        docker.contains("package.pop(\"version\", None)")
            && docker.contains("package.pop(\"registryBaseUrl\", None)")
            && docker.contains("distribution[\"ociImage\"] = image"),
        "Docker/MCP registry workflow must emit a canonical OCI package without forbidden legacy fields"
    );
    let registry = workflow_job_block(&docker, "registry");
    assert!(
        registry.contains("mcp-publisher publish"),
        "the product-specific MCP Registry publication must remain in Soma"
    );
    assert!(
        docker.contains("hosted-container-release.yml@542ea7b7e5ca2d4e21f3277bfcf158584fee90ec"),
        "container publication must use the pinned fleet workflow"
    );
}
