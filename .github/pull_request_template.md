## What & why

<!-- What does this change and why? Link any related issue: Closes #123 -->

## How tested

<!-- Commands you ran. CI runs: cargo fmt --check, clippy -D warnings, cargo test --all, web build. -->

## Checklist

- [ ] `cargo fmt --all` clean
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo test --all` passes
- [ ] Web UI builds (`cd web && npm run build`) if touched
- [ ] No secrets / `.env` / DB files committed
