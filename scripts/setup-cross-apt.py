#!/usr/bin/env python3
"""Configure Ubuntu apt for amd64 + arm64 cross-compilation."""

from __future__ import annotations

import pathlib
import subprocess
import sys


def main() -> int:
    print("Configuring multi-arch apt sources for arm64 cross-compilation...")

    try:
        subprocess.run(["dpkg", "--add-architecture", "arm64"], check=True)
    except subprocess.CalledProcessError as error:
        print(f"Error adding arm64 architecture: {error}", file=sys.stderr)
        return 1

    sources_path = pathlib.Path("/etc/apt/sources.list.d/ubuntu.sources")
    if not sources_path.exists():
        print(
            "Warning: /etc/apt/sources.list.d/ubuntu.sources not found. "
            "Skipping apt sources restriction."
        )
        return 0

    print("Restricting existing ubuntu.sources to amd64...")
    content = sources_path.read_text(encoding="utf-8")
    stanzas = content.split("\n\n")
    new_stanzas = []
    suites: set[str] = set()
    for stanza in stanzas:
        if not stanza.strip():
            continue
        lines = stanza.strip().split("\n")
        is_source = any(line.strip().startswith("Types:") for line in lines)
        if is_source and not any(
            line.strip().startswith("Architectures:") for line in lines
        ):
            lines.append("Architectures: amd64")
        new_stanzas.append("\n".join(lines))
        for line in lines:
            if line.strip().startswith("Suites:"):
                for suite in line.split(":", 1)[1].strip().split():
                    suites.add(suite)
    sources_path.write_text("\n\n".join(new_stanzas) + "\n", encoding="utf-8")

    if suites:
        suites_str = " ".join(sorted(suites))
        print(f"Creating arm64-ports.sources with suites: {suites_str}...")
        ports_content = f"""Types: deb
URIs: http://ports.ubuntu.com/ubuntu-ports/
Suites: {suites_str}
Components: main restricted universe multiverse
Architectures: arm64
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
"""
        pathlib.Path("/etc/apt/sources.list.d/arm64-ports.sources").write_text(
            ports_content,
            encoding="utf-8",
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
