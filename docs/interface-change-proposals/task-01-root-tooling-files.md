# Task 01 proposal: root TypeScript tooling files

## Status

Proposed for integration-owner acceptance.

## Reason

Task 01 owns the root `package.json`, `pnpm-workspace.yaml`, lockfile, and seven TypeScript packages, but the ownership list does
not explicitly name the configuration files required to lint and typecheck that workspace.

## Added files

- `.gitignore`: excludes dependencies, compiled output, coverage, runtime databases, and local environment secrets.
- `tsconfig.json`: strict shared compiler policy and workspace source paths.
- `biome.json`: one formatter/linter policy for task 01 TypeScript and fixtures.

These files contain no Rust, UI, JIR/JDP domain type, or other session's configuration. Removing them would make the required
`pnpm lint` and `pnpm typecheck` commands non-reproducible. The integration owner should either accept these names or move the
same settings into an agreed root tooling convention before merge.
