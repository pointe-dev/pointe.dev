# Dev workflow — branch → PR → (staging) → main

How a change ships. The merge gate is **convention-only today** (GitHub paywalls
branch protection on free private repos); `scripts/enable-branch-protection.sh`
makes it server-enforced the moment the repo goes Pro or public.

## The flow

```
feature branch ──open PR (targets main)
      │
      ├─ (optional) add the `staging` label
      │     └─► deploys THAT commit to https://staging.pointe.dev + runs the smoke
      │         (see docs/STAGING.md)  ──► eyeball / approve
      │
      └─ merge the PR ──► main ──► prod deploy (deploy.yml)
```

There is **no long-lived staging branch**. A branch is never merged into an
intermediate branch — it stays a branch until the final merge to `main`. Staging
tests the *exact commit* that will land on main, so nothing is merged twice and
staging can't drift from main.

## The merge rule (self-discipline until enforced)

Merge a PR to `main` only when **both** hold:

1. **CI is green** — `Unit Tests`, `Integration Tests`, `Build Docker Image`
   all pass on the PR. (`Coverage` is informational, not a gate.)
2. **If the change touches the funnel or runtime behaviour** — label it
   `staging`, confirm the staging smoke passed, and eyeball
   `https://staging.pointe.dev`. Pure docs/CI/config changes can skip this.

Ignore the `Workers Builds: pointe-dev` check — it's a stale Cloudflare Workers
integration that fails on every commit (including `main`), unrelated to our CI.
Disconnect it in the Cloudflare dashboard when convenient.

## Making the gate server-enforced

Convention relies on discipline; to make GitHub *block* a non-conforming merge:

1. Upgrade to GitHub Pro (keeps the repo private) **or** make the repo public.
2. Run:
   ```bash
   gh auth status            # must be an admin of pointe-dev/pointe.dev
   bash scripts/enable-branch-protection.sh
   ```

That creates a ruleset on `main`: no direct pushes (PR required), no force-push
or deletion, and the three CI checks must be green before merge. Required *human*
approval is left at 0 because GitHub forbids approving your own PR — add a second
collaborator and bump `required_approving_review_count` in the script to require
a reviewer.
