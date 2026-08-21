#!/usr/bin/env python3
"""Pin release ordering at the irreversible tag and registry boundaries."""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
REGISTRY_VERIFIER = ROOT / "scripts" / "verify-npm-release.py"
REGISTRY_READBACK = 'python3 scripts/verify-npm-release.py "${ESTELLE_RELEASE_TAG#v}"'


def assert_release_contract(workflow: str, verifier: str) -> None:
    assert "workflow_dispatch:" in workflow, "release must start from a validated candidate"
    assert 'tags:\n      - "v*"' not in workflow, "a tag must not start its own validation"
    assert "release_tag:" in workflow, "dispatch must name the intended immutable tag"
    assert "ESTELLE_RELEASE_TAG: ${{ inputs.release_tag }}" in workflow
    assert "timeout-minutes: 120" in workflow, "native build budget must cover measured Intel tail"
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
        "short native budget": workflow.replace("timeout-minutes: 120", "timeout-minutes: 90", 1),
        "missing registry read-back": workflow.replace(REGISTRY_READBACK, "", 1),
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
    print("release pipeline proof: dispatch-before-tag, 120m build, remote npm artifact read-back")


if __name__ == "__main__":
    main()
