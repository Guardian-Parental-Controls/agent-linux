#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:?Rust target triple, e.g. x86_64-unknown-linux-gnu}"
OUTPUT_DIR="${2:?Output directory for release assets}"

ARCHIVE_NAME="guardian-agent-${TARGET}.tar.gz"
BINARY="target/${TARGET}/release/guardian-agent"
STAGING="${OUTPUT_DIR}/staging-${TARGET}"

mkdir -p "${STAGING}"
cp "${BINARY}" "${STAGING}/guardian-agent"
chmod 0755 "${STAGING}/guardian-agent"

mkdir -p "${OUTPUT_DIR}"
tar -czf "${OUTPUT_DIR}/${ARCHIVE_NAME}" -C "${STAGING}" guardian-agent
sha256sum "${OUTPUT_DIR}/${ARCHIVE_NAME}" | awk '{print $1}' > "${OUTPUT_DIR}/${ARCHIVE_NAME}.sha256"
