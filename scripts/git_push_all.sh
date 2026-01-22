#!/usr/bin/env bash
set -e

echo "========================================"
echo "[git] DCMS push script"
echo "========================================"

# Ensure we're in a git repo
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "ERROR: Not inside a git repository"
  exit 1
fi

# Show current status
echo
echo "[git] Current status:"
git status

echo
read -p "[git] Continue with add + commit + push? (y/N): " confirm
if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
  echo "[git] Aborted."
  exit 0
fi

# Add everything
echo
echo "[git] Adding all changes..."
git add -A

# Commit message (auto + timestamp)
COMMIT_MSG="feat(auth): session extensions, impersonation, tests, fixes ($(date '+%Y-%m-%d %H:%M'))"

echo
echo "[git] Committing with message:"
echo "  $COMMIT_MSG"
git commit -m "$COMMIT_MSG" || {
  echo "[git] Nothing to commit."
  exit 0
}

# Push
echo
echo "[git] Pushing to origin..."
git push origin HEAD

echo
echo "[git] ✅ Push complete"
