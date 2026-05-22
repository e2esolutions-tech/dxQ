# Restoring the GitHub Actions workflows

These files live here instead of `.github/workflows/` because
the repo's current GitHub OAuth token doesn't carry the
`workflow` scope. Pushing files under `.github/workflows/`
without that scope is rejected by GitHub's API — precedent:
commit `69ac7e3`.

## One-time restore

```bash
# 1. Refresh the gh token so it carries the workflow scope.
gh auth refresh -h github.com -s workflow

# 2. Move the staged files into place.
mkdir -p .github/workflows
git mv dist/github-workflows/ci.yml      .github/workflows/ci.yml
git mv dist/github-workflows/release.yml .github/workflows/release.yml
git mv dist/github-workflows/README.md   .github/workflows/README.md
# RESTORE.md can stay here or be deleted — the workflow files
# themselves carry the documentation.

# 3. Commit + push.
git commit -m "ci: restore GitHub Actions workflows (after gh token refresh)"
git push github main
```

After the push lands, CI fires on the next push/PR and the
`release` workflow fires when you tag a release.

## What's in each file

- `ci.yml` — `cargo check / clippy / fmt / test` on stable +
  MSRV, web build (TypeScript + Vite + 300 KB gzip budget
  check), paper build (Pandoc + WeasyPrint + PDFs as
  artifact).
- `release.yml` — fires on `v*` tags. Runs
  `make packages-{deb,rpm}` and the paper submission
  bundler, publishes everything as a GitHub Release asset
  with a `SHA256SUMS` manifest.
- `README.md` — operator-facing pipeline docs.
