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

# Get commit message
echo
read -p "[git] Enter commit message (press Enter for default): " custom_msg

if [[ -z "$custom_msg" ]]; then
  COMMIT_MSG="Updated ($(date '+%Y-%m-%d %H:%M'))"
  echo "[git] Using default message:"
else
  COMMIT_MSG="$custom_msg"
  echo "[git] Using custom message:"
fi

echo "  $COMMIT_MSG"

# Commit
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