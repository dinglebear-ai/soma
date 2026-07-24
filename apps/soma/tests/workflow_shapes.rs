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
            .contains("googleapis/release-please-action@5c625bfb5d1ff62eadeeb3772007f7f66fdcf071"),
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
            workflow.contains("workflow_dispatch:"),
            "artifact workflow must support manual reruns for existing tags"
        );
        assert!(
            workflow.contains("validate-release-tag:"),
            "manual artifact reruns must validate the requested release tag before publishing"
        );
        assert!(
            workflow.contains(r#"refs/tags/${tag}^{commit}"#)
                && workflow.contains("git merge-base --is-ancestor")
                && workflow.contains("cargo xtask check-version-sync"),
            "artifact workflows must reject non-tag refs, tags outside main, and version drift"
        );
    }
    assert!(
        release.contains("tag_name: ${{ needs.validate-release-tag.outputs.release_tag }}"),
        "release artifact workflow must attach files to the existing release tag"
    );
    let lfs_commit = workflow_job_block(&release, "lfs-commit");
    assert!(
        lfs_commit.contains("ref: main") && lfs_commit.contains("git merge --ff-only origin/main"),
        "LFS binary commits must be made on top of current main, not from a detached release tag"
    );
    let npm = workflow_job_block(&release, "npm");
    assert!(
        npm.contains("needs: [validate-release-tag, release]"),
        "npm publish must wait until GitHub release artifacts have been created"
    );
    assert!(
        release.contains("arch: linux-x86_64")
            && release.contains("artifacts/${BIN}-linux-x86_64.tar.gz"),
        "release assets must include the installer's linux-x86_64 naming convention"
    );
    assert!(
        docker.contains("package.pop(\"version\", None)")
            && docker.contains("package.pop(\"registryBaseUrl\", None)")
            && docker.contains("distribution[\"ociImage\"] = image"),
        "Docker/MCP registry workflow must emit a canonical OCI package without forbidden legacy fields"
    );
    assert!(
        docker.contains("io.modelcontextprotocol.server.name=ai.dinglebear/soma"),
        "published images must carry the MCP Registry ownership label"
    );
    let registry = workflow_job_block(&docker, "registry");
    assert!(
        registry.contains("github.event_name == 'workflow_dispatch' && github.sha"),
        "manual recovery runs must use the current manifest while publishing the requested release tag"
    );
    assert!(
        docker.contains("retire_legacy_registry_name:")
            && registry.contains("inputs.retire_legacy_registry_name")
            && registry.contains("ai.dinglebear/soma-rmcp"),
        "manual recovery runs must expose an explicit, scoped path to retire Soma's legacy Registry name"
    );
    assert!(
        registry.contains("else\n            ./mcp-publisher publish\n          fi"),
        "legacy retirement mode must not republish an already-existing canonical version"
    );
}
