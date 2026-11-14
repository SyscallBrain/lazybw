## What and why

<!-- What does this change, and what problem does it solve?
     If it fixes an issue, write "Closes #123". -->

## Checklist

- [ ] `scripts/check.sh` passes
- [ ] Tests cover the change (snapshots updated if the interface changed)
- [ ] `book/src/reference/` and the rest of the book are updated if
      keys, options or behaviour changed
- [ ] No secrets in arguments, logs or `Debug` output
- [ ] Commit messages follow Conventional Commits (`fix:`, `feat:`, `docs:` …)
- [ ] For a performance change: before and after numbers from
      `cargo run --release --example bench`
