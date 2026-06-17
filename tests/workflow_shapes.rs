fn workflow_job_block<'a>(workflow: &'a str, job_name: &str) -> &'a str {
    let marker = format!("  {job_name}:");
    let start = workflow
        .find(&marker)
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
    &rest[..end]
}

#[test]
fn ci_runs_release_version_gate_before_merge() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let template = workflow_job_block(workflow, "template");
    assert!(
        template.contains(
            "cargo xtask check-release-versions --base origin/main --head HEAD --mode pr"
        ),
        "CI must run the manifest-backed release version gate on pull requests"
    );
    assert!(
        template.contains("fetch-depth: 0"),
        "release version gate needs tags and history"
    );
}

#[test]
fn auto_tag_uses_xtask_release_plan() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    let plan = workflow_job_block(workflow, "plan");
    let release = workflow_job_block(workflow, "release");
    assert!(
        plan.contains("cargo xtask release-plan --head HEAD --mode main --json"),
        "auto-tag must use the shared xtask release-version detector"
    );
    assert!(
        plan.contains("fetch-depth: 0"),
        "auto-tag release planning needs tag history"
    );
    assert!(
        plan.contains("persist-credentials: false"),
        "read-only plan checkout should not persist write credentials"
    );
    assert!(
        plan.contains(
            "matrix=$(jq -c '{include: [.[] | select(.changed == true)]}' release-plan.json)"
        ),
        "auto-tag matrix must include only changed components"
    );
    assert!(
        release.contains(r#"needs.plan.outputs.matrix != '{"include":[]}'"#),
        "auto-tag must skip release job for an empty matrix"
    );
    assert!(
        release.contains("fromJson(needs.plan.outputs.matrix)"),
        "auto-tag must expand the xtask plan as a matrix"
    );
    assert!(
        release.contains("matrix.candidate_tag") && release.contains("matrix.version"),
        "auto-tag must consume tags and versions from the xtask release plan"
    );
    assert!(
        release
            .find("Wait for CI to pass on this commit")
            .expect("CI wait step")
            < release.find("Create and push tag").expect("tag step"),
        "auto-tag must wait for CI before creating release tags"
    );
    for required in [
        "--branch main",
        "--event push",
        ".headSha == $sha",
        ".event == \"push\"",
        ".headBranch == \"main\"",
    ] {
        assert!(
            release.contains(required),
            "auto-tag CI polling must constrain {required}"
        );
    }
}
