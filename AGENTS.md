# Jingwei Agent repository policy

## Canonical workspace

- The only canonical Jingwei Agent repository is `D:\Projects\CD041_LUBAN\jingwei-agent`.
- Run all Jingwei Agent development, review, testing, packaging, Git commits, and pushes from this directory.
- Put Windows release executables and their verification evidence under `D:\Projects\CD041_LUBAN\jingwei-agent\dist`.
- Do not create or use sibling or nested repositories/worktrees such as `jingwei-agent-integration`, `jingwei-agent-task*`, or any other separately named Jingwei Agent folder.
- If isolation is needed, use branches inside this repository. Preserve unrelated user changes and keep task outputs in their owned paths.

## Product naming

- `Jingwei` refers to the Chinese mythological bird 精卫, not 经纬.
