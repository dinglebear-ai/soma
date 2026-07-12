fn workflow_job_block(workflow: &str, job_name: &str) -> String {
    let workflow = workflow.replace("\r\n", "\n").replace('\r', "\n");
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
            .contains("googleapis/release-please-action@8b8fd2cc23b2e18957157a9d923d75aa0c6f6ad5"),
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
    let release = include_str!("../../../.github/workflows/release.yml");
    let docker = include_str!("../../../.github/workflows/docker-publish.yml");
    for workflow in [release, docker] {
        assert!(
            workflow.contains("release:\n    types: [published]"),
            "artifact workflow must trigger from release-please published releases"
        );
        assert!(
            workflow.contains("workflow_dispatch:"),
            "artifact workflow must support manual reruns for existing tags"
        );
    }
    assert!(
        release.contains("tag_name: ${{ env.RELEASE_TAG }}"),
        "release artifact workflow must attach files to the existing release tag"
    );
    assert!(
        docker.contains("distribution[\"ociImage\"] = image"),
        "Docker/MCP registry workflow must rewrite the nested publisher OCI image"
    );
}
