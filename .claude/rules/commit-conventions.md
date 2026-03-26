# Commit Conventions

## Format

Conventional Commits: `<type>[optional scope]: <description> [optional (#PR)]`

Lowercase description (except proper nouns). No period at end.

## Prefixes Used

- `feat:` — new features or capabilities
- `fix:` — bug fixes
- `build:` — build system changes (non-dependency)
- `build(deps):` — dependency version bumps
- `chore:` — maintenance (clippy fixes, dep updates, version bumps, MSRV)
- `docs:` — documentation changes

## Scopes

- `deps` — dependency bumps (always paired with `build`)
- No other scopes are regularly used; most commits are unscoped

## PR Conventions

- CI tests on macOS, Linux (x86_64, i686, aarch64), and Windows
- Changes must be cross-platform; Windows uses `directories-next` for config paths, Unix uses `xdg`
- Dependabot PRs follow: `build(deps): bump <crate> from <old> to <new> (#<PR>)`

## Co-authoring

When Claude authors or co-authors a commit, include a blank line then:

```
Co-Authored-By: Claude <noreply@anthropic.com>
```
