#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "Usage: $0 [--dry-run] <new-version>"
    echo "  e.g. $0 0.14.0"
    echo "       $0 --dry-run 0.14.0"
    echo ""
    echo "Updates version in all workspace manifests, binding deps, and CHANGELOG."
    echo ""
    echo "Options:"
    echo "  --dry-run   Show what would change without modifying any files"
    exit 1
}

DRY_RUN=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN=true; shift ;;
        -h|--help) usage ;;
        -*) echo "Unknown option: $1"; usage ;;
        *) break ;;
    esac
done

[[ $# -ne 1 ]] && usage

NEW_VERSION="$1"

if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Error: '$NEW_VERSION' is not a valid semver version"
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# Detect sed flavor (macOS BSD vs GNU).
if sed --version 2>/dev/null | grep -q GNU; then
    sedi() { sed -i "$@"; }
else
    sedi() { sed -i '' "$@"; }
fi

# Read current version from workspace root.
CURRENT=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Bumping $CURRENT → $NEW_VERSION"
echo ""

if $DRY_RUN; then
    echo "Dry run — files that would be modified:"
    echo "  Cargo.toml"
    echo "  matcher_py/pyproject.toml"
    echo "  matcher_java/pom.xml"
    echo "  matcher_py/Cargo.toml"
    echo "  matcher_java/Cargo.toml"
    echo "  matcher_c/Cargo.toml"
    echo "  matcher_java/README.md"
    echo "  CHANGELOG.md (new section: $NEW_VERSION - $(date +%Y-%m-%d))"
    echo ""
    echo "No files were changed."
    exit 0
fi

# 1. Workspace root Cargo.toml
sedi "s/^version = \"$CURRENT\"/version = \"$NEW_VERSION\"/" Cargo.toml

# 2. Python pyproject.toml
sedi "s/^version = \"$CURRENT\"/version = \"$NEW_VERSION\"/" matcher_py/pyproject.toml

# 3. Java pom.xml (first <version> after <artifactId>matcher_java)
sedi "s|<version>$CURRENT</version>|<version>$NEW_VERSION</version>|" matcher_java/pom.xml

# 4-6. Binding Cargo.toml dependency pins
for f in matcher_py/Cargo.toml matcher_java/Cargo.toml matcher_c/Cargo.toml; do
    sedi "s/version = \"$CURRENT\"/version = \"$NEW_VERSION\"/g" "$f"
done

# 7. Java README Maven coordinates
sedi "s|<version>$CURRENT</version>|<version>$NEW_VERSION</version>|" matcher_java/README.md

# 8. CHANGELOG — insert new section header after line 1
TODAY=$(date +%Y-%m-%d)
sedi "2a\\
\\
## $NEW_VERSION - $TODAY" CHANGELOG.md

echo "Updated files:"
git diff --stat
echo ""
echo "Review with: git diff"
echo "To revert:   git checkout ."
