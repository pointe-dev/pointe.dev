#!/usr/bin/env bash
# Make the main-branch merge gate server-enforced.
#
# REQUIRES one of: GitHub Pro on pointe-dev (keeps the repo private) OR a public
# repo. On a free private repo GitHub returns 403 and nothing is enforced — see
# docs/WORKFLOW.md (until then the gate is convention-only).
#
# Creates a ruleset on `main`:
#   - PR required (no direct pushes to main)
#   - no force-push, no branch deletion
#   - these CI checks must be green before merge, branch must be up to date
#
# Idempotent: deletes any prior ruleset of the same name, then recreates it.
#
# Usage:  gh auth status   # must be an admin of the repo
#         bash scripts/enable-branch-protection.sh
set -euo pipefail

REPO="${REPO:-pointe-dev/pointe.dev}"
RULESET_NAME="main protection"

command -v gh >/dev/null || { echo "✗ gh CLI required"; exit 1; }

# Drop an existing ruleset with the same name so re-runs don't pile up.
existing=$(gh api "repos/$REPO/rulesets" --jq \
  ".[] | select(.name == \"$RULESET_NAME\") | .id" 2>/dev/null || true)
if [ -n "$existing" ]; then
  echo "↻ removing existing ruleset $existing"
  gh api -X DELETE "repos/$REPO/rulesets/$existing"
fi

echo "▶ creating ruleset '$RULESET_NAME' on main of $REPO"
gh api -X POST "repos/$REPO/rulesets" --input - <<'JSON'
{
  "name": "main protection",
  "target": "branch",
  "enforcement": "active",
  "conditions": {
    "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] }
  },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    {
      "type": "pull_request",
      "parameters": {
        "required_approving_review_count": 0,
        "dismiss_stale_reviews_on_push": true,
        "require_code_owner_review": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": false
      }
    },
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": true,
        "required_status_checks": [
          { "context": "Unit Tests" },
          { "context": "Integration Tests" },
          { "context": "Build Docker Image" }
        ]
      }
    }
  ]
}
JSON

echo "✅ done. main now requires a PR + green CI before merge."
echo "   (To require a human reviewer, add a 2nd collaborator and set"
echo "    required_approving_review_count to 1 in this script, then re-run.)"
