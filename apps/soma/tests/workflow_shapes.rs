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
fn shared_workflow_callers_use_approved_reachable_revisions() {
    const WORKFLOW_PREFIX: &str = "dinglebear-ai/workflows/.github/workflows/";
    const FLEET_REVISION: &str = "ac57c3208cf92d71c5971bb936df51c400cb1ccf";
    // npm publication needs token-mode support added after the fleet revision.
    const NPM_PUBLISH_REVISION: &str = "64d705af6e164aac58d507df6fb2f6bdc8a4d22d";
    // Native wheels need platform-specific cibuildwheel architecture names.
    const PYTHON_WHEELS_REVISION: &str = "eadba32f019e984b26d93c807ef72e5094df2876";
    let workflow_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".github/workflows");
    let mut unexpected_callers = Vec::new();
    let mut caller_count = 0;
    let mut implementation_ref_count = 0;

    for entry in std::fs::read_dir(&workflow_dir).expect("read workflow directory") {
        let path = entry.expect("read workflow entry").path();
        if !matches!(
            path.extension().and_then(std::ffi::OsStr::to_str),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let workflow = std::fs::read_to_string(&path).expect("read workflow file");
        for (line_number, line) in workflow.lines().enumerate() {
            if let Some(revision) = line.trim().strip_prefix("implementation-ref:") {
                implementation_ref_count += 1;
                if revision.trim() != FLEET_REVISION {
                    unexpected_callers.push(format!(
                        "{}:{}: implementation-ref {}",
                        path.display(),
                        line_number + 1,
                        revision.trim()
                    ));
                }
            }
            let Some((_, shared_call)) = line.split_once(WORKFLOW_PREFIX) else {
                continue;
            };
            let Some((called_workflow, revision)) = shared_call.split_once('@') else {
                unexpected_callers.push(format!(
                    "{}:{}: missing immutable revision",
                    path.display(),
                    line_number + 1
                ));
                continue;
            };
            caller_count += 1;
            let expected_revision = match called_workflow {
                "npm-trusted-publish.yml" => NPM_PUBLISH_REVISION,
                "hosted-python-wheels.yml" => PYTHON_WHEELS_REVISION,
                _ => FLEET_REVISION,
            };
            if revision.split_whitespace().next() != Some(expected_revision) {
                unexpected_callers.push(format!(
                    "{}:{}: {called_workflow}@{revision}",
                    path.display(),
                    line_number + 1
                ));
            }
        }
    }

    assert!(caller_count > 0, "expected shared workflow callers");
    assert!(
        implementation_ref_count > 0,
        "expected shared contract implementation revisions"
    );
    unexpected_callers.sort();
    assert!(
        unexpected_callers.is_empty(),
        "shared workflow callers must use approved default-branch revisions:\n{}",
        unexpected_callers.join("\n")
    );
}

#[test]
fn hosted_container_smoke_is_explicitly_allowed_by_fleet_policy() {
    let ci = include_str!("../../../.github/workflows/ci.yml");
    let fleet_policy = include_str!("../../../.github/workflows/fleet-policy.yml");
    let container_smoke = workflow_job_block(ci, "container-smoke");
    let policy = workflow_job_block(fleet_policy, "policy");

    assert!(
        container_smoke.contains("runs-on: ubuntu-24.04"),
        "production container smoke must retain a hosted Docker runner"
    );
    assert!(
        policy.contains("allow-hosted-fast: true"),
        "fleet policy must explicitly allow the hosted production container smoke"
    );
}

#[test]
fn container_hot_reload_source_stays_outside_service_owned_data() {
    let ci = include_str!("../../../.github/workflows/ci.yml");
    let container_smoke = workflow_job_block(ci, "container-smoke");

    assert!(
        container_smoke.contains("provider_dir=\"${RUNNER_TEMP}/soma-container-providers\""),
        "hot-reloaded providers must live outside /data because the entrypoint recursively owns /data"
    );
    assert!(
        container_smoke.contains("-e SOMA_PROVIDER_DIR=/providers")
            && container_smoke.contains("-v \"${provider_dir}:/providers:ro\""),
        "the production-container smoke must mount the externally mutable provider source separately"
    );
    assert!(
        !container_smoke.contains("${data_dir}/providers"),
        "the host runner cannot hot-reload files under the service-owned /data bind mount"
    );
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
fn frontend_ci_runs_the_web_contract_tests() {
    let workflow = include_str!("../../../.github/workflows/ci.yml");
    let frontend = workflow_job_block(workflow, "frontend-assets");

    assert!(
        frontend.contains("test-command: pnpm test"),
        "frontend CI must run the action and OpenAPI parity tests, not only build assets"
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
            && release.contains("workflow_dispatch:")
            && release.contains("tag_name:")
            && release
                .contains("ref: refs/tags/${{ github.event.release.tag_name || inputs.tag_name }}")
            && !release.contains("self-hosted"),
        "heavy native builds must use published releases or an explicit immutable-tag recovery on GitHub-hosted runners"
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
    }
    assert!(
        release.contains("workflow_dispatch:")
            && release.contains("tag_name:")
            && release.contains("release_ref=refs/tags/${tag}")
            && release
                .contains("checkout-ref: ${{ needs.validate-release-tag.outputs.release_ref }}"),
        "the binary release workflow must support only explicit immutable-tag recovery"
    );
    assert!(
        docker.contains("workflow_dispatch:")
            && docker.contains("tag_name:")
            && docker.contains("release_ref=refs/tags/${RELEASE_TAG}")
            && docker.contains("checkout-ref: ${{ needs.validate.outputs.release_ref }}"),
        "the container publication workflow must support explicit immutable-tag recovery"
    );
    assert!(
        release.contains("validate-release-tag:") && docker.contains("validate:"),
        "artifact publication must validate the immutable release contract"
    );
    let docker_validate = workflow_job_block(&docker, "validate");
    assert!(
        docker_validate
            .contains("if: startsWith(github.event.release.tag_name || inputs.tag_name, 'v')"),
        "the Soma container workflow must ignore component-prefixed provider releases"
    );
    assert!(
        release.contains("gh release upload")
            && release.contains("\"${{ needs.validate-release-tag.outputs.release_tag }}\""),
        "release artifact workflow must attach files to the existing release tag"
    );
    assert!(
        release.contains("sudo apt-get install -y pkg-config libssl-dev libseccomp-dev"),
        "release builds must install the native seccomp development library"
    );
    assert!(
        docker.contains("sudo apt-get install -y libseccomp-dev"),
        "container release validation must install the native seccomp development library"
    );
    assert!(
        !release.contains("git push origin HEAD:main") && !release.contains("ref: main"),
        "release artifact workflow must not write generated binaries back to main"
    );
    let npm = workflow_job_block(&release, "npm");
    assert!(
        npm.contains("needs: [validate-release-tag, release]")
            && npm.contains("npm-trusted-publish.yml@64d705af6e164aac58d507df6fb2f6bdc8a4d22d"),
        "npm publish must wait for artifacts and use the fleet source of truth"
    );
    assert!(
        !npm.contains("NPM_TOKEN"),
        "trusted npm publication must use OIDC without a legacy registry token"
    );
    assert!(
        release.contains("arch: linux-x86_64")
            && release.contains("artifacts/${{ env.BINARY_NAME }}-${{ matrix.arch }}.tar.gz",),
        "release assets must include the installer's linux-x86_64 naming convention"
    );
    let registry =
        normalize_workflow_newlines(include_str!("../../../.github/workflows/mcp-registry.yml"));
    assert!(
        !docker.contains("mcp-publisher") && !docker.contains("registry.modelcontextprotocol.io"),
        "container publication must not duplicate the shared MCP Registry publisher"
    );
    assert!(
        registry.contains("release:")
            && registry.contains("types: [published]")
            && registry.contains("workflow_dispatch:")
            && registry.contains("expected-version:")
            && registry.contains("manifest-path: server.json"),
        "MCP Registry publication must support release events and explicit recovery"
    );
    assert!(
        registry.contains("mcp-registry-publish.yml@3302f853574ba0c669a647f66cfcacb81f529fff")
            && registry.contains("auth-method: dns")
            && registry.contains("MCP_PRIVATE_KEY"),
        "MCP Registry publication must use the pinned fleet source of truth with DNS ownership"
    );
    assert!(
        docker.contains("hosted-container-release.yml@ac57c3208cf92d71c5971bb936df51c400cb1ccf"),
        "container publication must use the pinned fleet workflow"
    );
}

#[test]
fn python_wheel_publish_merges_platform_artifacts() {
    let workflow = include_str!("../../../.github/workflows/python-wheels.yml");

    assert!(
        workflow.contains("pattern: soma-provider-wheels-*")
            && workflow.contains("merge-multiple: true"),
        "provider publication must merge every platform-specific wheel artifact"
    );
}

#[test]
fn python_wheel_release_supports_immutable_tag_recovery() {
    let workflow = include_str!("../../../.github/workflows/python-wheels.yml");
    let release_tag = "${{ github.event.release.tag_name || inputs.tag_name }}";

    assert!(
        workflow.contains("workflow_dispatch:") && workflow.contains("tag_name:"),
        "provider wheels need an explicit immutable-tag recovery input"
    );
    assert!(
        workflow.matches(release_tag).count() >= 4,
        "provider build and publish jobs must consistently use the resolved release tag"
    );
    assert!(
        workflow.contains("test \"${RELEASE_TAG}\" = \"soma-provider-v${version}\"")
            && workflow.contains("gh release upload \"${RELEASE_TAG}\""),
        "provider recovery must validate and upload to the requested immutable tag"
    );
}
#[test]
fn python_wheel_release_uses_oidc_capable_pypi_publisher() {
    let workflow = include_str!("../../../.github/workflows/python-wheels.yml");

    assert!(
        workflow.contains("pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33",)
            && workflow.contains("packages-dir: dist")
            && workflow.contains("attestations: true"),
        "provider publication must use the approved OIDC and attestation capable PyPI action"
    );
}
