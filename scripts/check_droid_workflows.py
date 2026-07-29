#!/usr/bin/env python3
"""Validate Droid workflow configuration and safety defaults."""

from __future__ import annotations

import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent


def _read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _error(message: str, errors: list[str]) -> None:
    errors.append(message)


def _has_mutable_factory_ref(text: str) -> bool:
    return "Factory-AI/droid-action@main" in text or "Factory-AI/droid-action@v" in text


def _has_safe_pinned_ref(text: str) -> bool:
    match = re.search(r"uses:\s*EffortlessMetrics/droid-action-safe@([0-9a-f]{40})", text)
    return match is not None


def _contains_text(text: str, pattern: str) -> bool:
    return pattern in text


def main() -> int:
    droid = REPO_ROOT / ".github" / "workflows" / "droid.yml"
    droid_review = REPO_ROOT / ".github" / "workflows" / "droid-review.yml"
    errors: list[str] = []

    droid_text = _read(droid)
    droid_review_text = _read(droid_review)

    for path, text in ((droid, droid_text), (droid_review, droid_review_text)):
        if _has_mutable_factory_ref(text):
            _error(f"{path}: still references mutable Factory-AI/droid-action@main/v", errors)
        if not _has_safe_pinned_ref(text):
            _error(f"{path}: does not use immutable EffortlessMetrics/droid-action-safe SHA ref", errors)

    if _contains_text(droid_text, "\n  issues:"):
        _error("droid.yml: still has issue event trigger", errors)

    required_commands = (
        "@droid review",
        "@droid fill",
        "@droid security",
    )
    droid_if_block = re.search(r"jobs:\s*\n\s*droid:\s*\n\s*if:\s*\|([\s\S]*?)\n\s*permissions:", droid_text)
    if not droid_if_block:
        _error("droid.yml: could not locate job if block", errors)
    else:
        if_text = droid_if_block.group(1)
        for command in required_commands:
            if command not in if_text:
                _error(f"droid.yml: missing explicit command gate '{command}' in if expression", errors)
        required_associations = ('OWNER","MEMBER","COLLABORATOR"')
        if required_associations not in if_text:
            _error("droid.yml: missing trusted author-association guard", errors)
        if "github.event.issue.pull_request" not in if_text:
            _error("droid.yml: issue_comment path does not require PR comments", errors)

    for key in ("upload_debug_artifacts: false", "show_full_output: false"):
        if key not in droid_text:
            _error(f"droid.yml: expected '{key}'", errors)
        if key not in droid_review_text:
            _error(f"droid-review.yml: expected '{key}'", errors)

    if "automatic_review: true" not in droid_review_text:
        _error("droid-review.yml: automatic_review is not enabled", errors)
    if "automatic_security_review: true" not in droid_review_text:
        _error("droid-review.yml: automatic_security_review is not enabled", errors)

    if "github.event.pull_request.head.repo.full_name == github.repository" not in droid_review_text:
        _error("droid-review.yml: missing same-repository guard for pull_request events", errors)

    if errors:
        print("droid workflow policy check failed:")
        for item in errors:
            print(f" - {item}")
        return 1

    print("droid workflow policy check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
