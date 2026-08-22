#!/usr/bin/env python3
"""Pin release ordering at the irreversible tag and registry boundaries."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
REGISTRY_VERIFIER = ROOT / "scripts" / "verify-npm-release.py"
REGISTRY_READBACK = 'python3 scripts/verify-npm-release.py "${ESTELLE_RELEASE_TAG#v}"'
NATIVE_TIMEOUT = "timeout-minutes: 75"
RESUMABLE_IDENTITY = '''remote_sha=$(git ls-remote origin "refs/tags/${ESTELLE_RELEASE_TAG}^{}" | awk '{print $1}')
          if test -n "$remote_sha"; then
            test "$remote_sha" = "$GITHUB_SHA"
          else
            test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)"
          fi'''


def assert_release_contract(workflow: str, verifier: str) -> None:
    assert "workflow_dispatch:" in workflow, "release must start from a validated candidate"
    assert 'tags:\n      - "v*"' not in workflow, "a tag must not start its own validation"
    assert "release_tag:" in workflow, "dispatch must name the intended immutable tag"
    assert "ESTELLE_RELEASE_TAG: ${{ inputs.release_tag }}" in workflow
    assert RESUMABLE_IDENTITY in workflow, (
        "an existing exact-SHA tag must remain rerunnable after main advances, while a new tag "
        "must still originate at current main"
    )
    validate_start = workflow.index("  validate:")
    build_start = workflow.index("  build:")
    release_start = workflow.index("  release:")
    validate = workflow[validate_start:build_start]
    native_build = workflow[build_start:release_start]
    assert "runs-on: ubuntu-24.04" in validate, "the cheap gate must run at Linux 1x billing"
    assert "Verify fork provenance and egress census" in validate
    assert "cargo fmt --all -- --check" in validate
    assert "cargo clippy --locked" in validate
    assert "cargo test --locked" in validate
    assert "needs: validate" in native_build, "native fan-out must wait for the cheap gate"
    assert "macos-" not in workflow[:build_start], "no metered macOS runner may precede validation"
    assert NATIVE_TIMEOUT in native_build, "native jobs must stop below the old 90-minute ceiling"
    assert "timeout-minutes: 90" not in native_build
    assert "timeout-minutes: 120" not in native_build
    tag_write = workflow.index('git push origin "refs/tags/${ESTELLE_RELEASE_TAG}"')
    release_write = workflow.index('gh release create "$ESTELLE_RELEASE_TAG"')
    assert tag_write < release_write, "release creation must use the post-gate immutable tag"
    assert 'git ls-remote origin "refs/tags/${ESTELLE_RELEASE_TAG}^{}"' in workflow
    assert REGISTRY_READBACK in workflow
    assert '"npm", "view"' in verifier, "registry version must be read back remotely"
    assert '"npm",\n            "pack"' in verifier, "customer tarball must be read back remotely"
    assert "EXPECTED_MEMBERS" in verifier, "tarball read-back must assert its exact surface"


def prove_contract_rejects_mutants(workflow: str, verifier: str) -> None:
    mutants = {
        "tag-first trigger": workflow.replace("workflow_dispatch:", "push:", 1),
        "native bypasses cheap gate": workflow.replace("needs: validate", "needs: []", 1),
        "metered validation runner": workflow.replace(
            "runs-on: ubuntu-24.04", "runs-on: macos-15-intel", 1
        ),
        "old 90-minute native ceiling": workflow.replace(NATIVE_TIMEOUT, "timeout-minutes: 90", 1),
        "stale manifest check moved out of gate": workflow.replace(
            "Verify fork provenance and egress census", "Verify provenance after native builds", 1
        ),
        "missing registry read-back": workflow.replace(REGISTRY_READBACK, "", 1),
        "tagged rerun tied to moving main": workflow.replace(
            RESUMABLE_IDENTITY,
            RESUMABLE_IDENTITY.replace(
                '          else\n            test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)"\n',
                '          test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)"\n          else\n',
            ),
            1,
        ),
    }
    for name, mutant in mutants.items():
        try:
            assert_release_contract(mutant, verifier)
        except (AssertionError, ValueError):
            continue
        raise AssertionError(f"release contract accepted mutant: {name}")


def main() -> None:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    verifier = REGISTRY_VERIFIER.read_text(encoding="utf-8") if REGISTRY_VERIFIER.exists() else ""
    assert_release_contract(workflow, verifier)
    prove_contract_rejects_mutants(workflow, verifier)
    print(
        "release pipeline proof: Linux gate before native fan-out, 75m native ceiling, "
        "dispatch-before-tag, exact-tag resume, remote npm artifact read-back"
    )


if __name__ == "__main__":
    main()
