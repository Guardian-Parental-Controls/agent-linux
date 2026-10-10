#!/usr/bin/env python3
"""Smoke-test linux release packaging and versions payload helpers."""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / "scripts" / "package-linux-release.sh"
PAYLOAD = ROOT / "scripts" / "versions-payload.py"


def run(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, check=True, text=True, capture_output=True)


def test_package_binary(tmp: Path) -> None:
    binary = tmp / "guardian-agent"
    binary.write_bytes(b"guardian-agent-test-binary")
    dist = tmp / "dist"
    run(
        [
            "bash",
            str(PACKAGE),
            "--binary",
            str(binary),
            "--name",
            "guardian-agent-x86_64-unknown-linux-gnu",
            "--dist",
            str(dist),
        ]
    )
    archive = dist / "guardian-agent-x86_64-unknown-linux-gnu.tar.gz"
    checksum = Path(str(archive) + ".sha256")
    assert archive.is_file(), archive
    assert checksum.is_file(), checksum
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    assert checksum.read_text(encoding="utf-8").strip() == digest
    with tarfile.open(archive, "r:gz") as handle:
        names = handle.getnames()
    assert names == ["guardian-agent"], names


def test_versions_payload(tmp: Path) -> None:
    dist = tmp / "feed-dist"
    dist.mkdir()
    for name in (
        "guardian-agent-x86_64-unknown-linux-gnu.tar.gz",
        "guardian-agent-aarch64-unknown-linux-gnu.tar.gz",
        "guardian-overlay-x86_64-unknown-linux-gnu.tar.gz",
    ):
        (dist / name).write_bytes(b"asset")
        (dist / f"{name}.sha256").write_text("abc\n", encoding="utf-8")

    output = tmp / "payload.json"
    run(
        [
            sys.executable,
            str(PAYLOAD),
            "--dist",
            str(dist),
            "--tag",
            "v1.0.1",
            "--output",
            str(output),
        ]
    )
    payload = json.loads(output.read_text(encoding="utf-8"))
    assert payload["event_type"] == "component-release"
    client = payload["client_payload"]
    assert client["component"] == "agent-linux"
    assert client["version"] == "1.0.1"
    ids = [artifact["id"] for artifact in client["artifacts"]]
    assert ids == ["linux-x86_64", "linux-aarch64", "linux-overlay-x86_64"]
    assert client["artifacts"][0]["url"].endswith(
        "/v1.0.1/guardian-agent-x86_64-unknown-linux-gnu.tar.gz"
    )
    assert client["install_script"].endswith("/v1.0.1/scripts/install-agent.sh")


def test_versions_payload_requires_both_arches(tmp: Path) -> None:
    dist = tmp / "partial"
    dist.mkdir()
    name = "guardian-agent-x86_64-unknown-linux-gnu.tar.gz"
    (dist / name).write_bytes(b"asset")
    result = subprocess.run(
        [sys.executable, str(PAYLOAD), "--dist", str(dist), "--tag", "v1.0.1"],
        text=True,
        capture_output=True,
    )
    assert result.returncode != 0
    assert "linux-aarch64" in result.stderr


def main() -> int:
    tmp = Path("/tmp/guardian-linux-release-helper-test")
    if tmp.exists():
        for child in sorted(tmp.rglob("*"), reverse=True):
            if child.is_file() or child.is_symlink():
                child.unlink()
            else:
                child.rmdir()
        tmp.rmdir()
    tmp.mkdir()
    test_package_binary(tmp)
    test_versions_payload(tmp)
    test_versions_payload_requires_both_arches(tmp)
    print("release helper tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
