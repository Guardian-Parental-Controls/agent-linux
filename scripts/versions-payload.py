#!/usr/bin/env python3
"""Build the versions repository_dispatch payload from published assets."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

COMPONENT = "agent-linux"
REPOSITORY = f"https://github.com/Guardian-Parental-Controls/{COMPONENT}"

ASSET_IDS = {
    "guardian-agent-x86_64-unknown-linux-gnu.tar.gz": "linux-x86_64",
    "guardian-agent-aarch64-unknown-linux-gnu.tar.gz": "linux-aarch64",
    "guardian-overlay-x86_64-unknown-linux-gnu.tar.gz": "linux-overlay-x86_64",
}
REQUIRED_IDS = {"linux-x86_64", "linux-aarch64"}


def build_payload(dist: Path, tag: str) -> dict:
    version = tag.lstrip("v")
    base = f"{REPOSITORY}/releases/download/{tag}"
    artifacts: list[dict[str, str]] = []

    for filename, artifact_id in ASSET_IDS.items():
        archive = dist / filename
        if not archive.is_file():
            continue
        artifact = {"id": artifact_id, "url": f"{base}/{filename}"}
        checksum = dist / f"{filename}.sha256"
        if checksum.is_file():
            artifact["checksum_url"] = f"{base}/{filename}.sha256"
        artifacts.append(artifact)

    present = {artifact["id"] for artifact in artifacts}
    missing = sorted(REQUIRED_IDS - present)
    if missing:
        raise SystemExit(f"missing required release artifacts: {', '.join(missing)}")

    return {
        "event_type": "component-release",
        "client_payload": {
            "component": COMPONENT,
            "version": version,
            "repository": REPOSITORY,
            "minimum_supported_version": "1.0.0",
            "min_server": "1.0.0",
            "protocol_version": 1,
            "install_script": (
                f"https://raw.githubusercontent.com/Guardian-Parental-Controls/"
                f"{COMPONENT}/{tag}/scripts/install-agent.sh"
            ),
            "artifacts": artifacts,
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dist", type=Path, default=Path("dist"))
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    payload = build_payload(args.dist, args.tag)
    encoded = json.dumps(payload, indent=2) + "\n"
    if args.output:
        args.output.write_text(encoded, encoding="utf-8")
    else:
        print(encoded, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
