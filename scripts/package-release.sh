#!/usr/bin/env bash
set -euo pipefail

# Package guardian-agent into a release tarball for the current Cargo target triple.
# Usage: package-release.sh <output-dir>
# Expects `cargo build --release --bin guardian-agent` to have been run for the active target.

OUT_DIR="${1:?output directory required}"
TARGET_TRIPLE="${TARGET_TRIPLE:-$(rustc -vV | sed -n 's/^host: //p')}"
ARCHIVE_NAME="guardian-agent-${TARGET_TRIPLE}.tar.gz"
STAGE="${OUT_DIR}/stage"
mkdir -p "${STAGE}"

BIN="target/${TARGET_TRIPLE}/release/guardian-agent"
if [[ ! -f "${BIN}" ]]; then
    BIN="target/release/guardian-agent"
fi
[[ -f "${BIN}" ]] || {
    echo "error: release binary not found for ${TARGET_TRIPLE}" >&2
    exit 1
}

install -m 0755 "${BIN}" "${STAGE}/guardian-agent"
tar -czf "${OUT_DIR}/${ARCHIVE_NAME}" -C "${STAGE}" guardian-agent
(
    cd "${OUT_DIR}"
    sha256sum "${ARCHIVE_NAME}" | awk '{print $1}' > "${ARCHIVE_NAME}.sha256"
)
echo "Packaged ${OUT_DIR}/${ARCHIVE_NAME}"
