#!/usr/bin/env bash
# Package a built guardian-agent (or overlay helper) for GitHub Releases.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  package-linux-release.sh --binary PATH --name BASENAME --dist DIR
  package-linux-release.sh --dir PATH --name BASENAME --dist DIR

Creates:
  DIR/BASENAME.tar.gz
  DIR/BASENAME.tar.gz.sha256   (hex digest only)
EOF
}

BINARY=""
BUNDLE_DIR=""
NAME=""
DIST=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary)
            [[ $# -ge 2 ]] || { usage >&2; exit 1; }
            BINARY="$2"
            shift 2
            ;;
        --dir)
            [[ $# -ge 2 ]] || { usage >&2; exit 1; }
            BUNDLE_DIR="$2"
            shift 2
            ;;
        --name)
            [[ $# -ge 2 ]] || { usage >&2; exit 1; }
            NAME="$2"
            shift 2
            ;;
        --dist)
            [[ $# -ge 2 ]] || { usage >&2; exit 1; }
            DIST="$2"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ -z "$NAME" || -z "$DIST" ]]; then
    usage >&2
    exit 1
fi
if [[ -n "$BINARY" && -n "$BUNDLE_DIR" ]]; then
    echo "Use either --binary or --dir, not both" >&2
    exit 1
fi
if [[ -z "$BINARY" && -z "$BUNDLE_DIR" ]]; then
    usage >&2
    exit 1
fi

mkdir -p "$DIST"
archive="${DIST}/${NAME}.tar.gz"
checksum="${archive}.sha256"

if [[ -n "$BINARY" ]]; then
    [[ -f "$BINARY" ]] || { echo "Binary not found: $BINARY" >&2; exit 1; }
    tar -czf "$archive" -C "$(dirname "$BINARY")" "$(basename "$BINARY")"
else
    [[ -d "$BUNDLE_DIR" ]] || { echo "Bundle directory not found: $BUNDLE_DIR" >&2; exit 1; }
    tar -czf "$archive" -C "$BUNDLE_DIR" .
fi

sha256sum "$archive" | awk '{print $1}' > "$checksum"
echo "Wrote ${archive} and ${checksum}"
