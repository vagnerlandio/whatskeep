@AGENTS.md

## Claude Code specifics

- Before changing something, open the matching recipe in [docs/RECIPES.md](docs/RECIPES.md) and follow the whole checklist (Rust + TS + tests + ACL).
- Before committing: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `pnpm lint:security` and `tsc --noEmit` in every TS package touched. If the export or the viewer changed: `pnpm test:e2e`.
- Commit only when the user asks; message in English.
- Never read, copy or paste the user's real backups or keys into external tools.
- The user writes in Portuguese; reply in Portuguese.
