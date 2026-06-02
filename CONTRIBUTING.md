# Contributing to Sezar

Sezar is pre-alpha. The V1 scope is documented in
[`ROADMAP.md`](ROADMAP.md) and the live punch list in
[`TODO.md`](TODO.md). Patches that move V1 items forward (or
file bugs against current behaviour) are the most valuable
contributions today.

## Before you open a PR

1. Follow the load-bearing project directives — including the
   **no AI attribution** rule and the **mandatory citation
   verification** for paper changes. Both predate any single
   contribution; please follow them.
2. Run the workspace checks locally:
   ```bash
   cargo check --workspace
   cargo test  --workspace
   ```
3. If you touched anything in `docs/paper/`, rebuild the PDFs
   and skim the patched Markdown in `docs/paper/build/` to
   confirm the citations render the way you intend:
   ```bash
   cd docs/paper && ./build.sh
   ```
4. If you touched anything that an operator deploys
   (`sezar-server`, `sezar-net`, `Dockerfile`, `compose.yaml`),
   run the end-to-end acceptance smoke:
   ```bash
   ./scripts/acceptance.sh
   ```

## Branches and commits

- Branch from `main`. Keep feature branches focused on one
  deliverable; split unrelated work.
- Commit message style: Conventional Commits.
  - `feat(scope): …`, `fix(scope): …`, `docs(scope): …`,
    `test(scope): …`, `chore(scope): …`, `refactor(scope): …`.
  - Scope is the crate name (`sezar-net`, `sezar-server`, …)
    or one of `paper`, `refs`, `ci`, `web`, `docker`, `scripts`,
    `repo`.
  - Imperative subject under ~70 chars; body explains the
    *why*, not the *what*.
- **No AI attribution anywhere** — no co-author trailers,
  no "generated with" footers, and no mention of any
  AI assistant or style in commit messages, code comments, PR
  descriptions, paper text, or anything that ships in the
  repo. This is a hard rule for all e2esolutions-tech repos
  and is enforced at review time.
- Sign-offs and `Co-Authored-By:` lines are welcome for
  *human* collaborators.

## Pull requests

- Open against `main`. The remote is named `github` in our
  local checkouts; for forks the standard `origin` is fine.
- Describe what the change does and how to verify it. A short
  test plan (commands + expected output) is enough for most
  PRs.
- If you change `docs/paper/references.bib` or any prose with
  citation keys, cross-check every touched entry against its
  live source page before requesting review. Fabricated author
  lists are the top failure mode for citation work — see
  commit `563ee87` for an example of how it can go wrong.
- If you change `crates/sezar-core/src/event.rs`, audit the
  schema-version impact and document what downstream consumers
  (`sezar-server`, the React UI types, `bindings/`) need to
  regenerate.
- One reviewer approval gets you merged. Squash-merge is the
  default.

## Roadmap discipline

If your change adds something to V1 that isn't already in
`ROADMAP.md`, update `ROADMAP.md` **before** the implementing
PR. Adding silently is not okay; deferring something explicitly
to make room for the new item is. Drift is fine; stealth-drift
is not.

## Reporting bugs

Open an issue at
<https://github.com/e2esolutions-tech/sezar/issues> with:

- exact command(s) you ran,
- expected vs observed behaviour,
- relevant log lines (`RUST_LOG=info` is the default; bump to
  `debug` for the affected crate when reporting),
- the commit hash (`git rev-parse --short HEAD`) and platform
  (Linux distro / kernel for sezar-net issues).

For security-sensitive reports, please email
<info@e2esolutions.tech> instead of filing a public issue.

## License

By contributing you agree that your work is released under the
MIT license carried by [`LICENSE`](LICENSE).
