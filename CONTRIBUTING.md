# Contributing to Dotify

Thanks for helping! This guide covers the workflow; the design lives in
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Setup

```bash
pnpm install
```

```bash
pnpm dev
```

## Before you open a pull request

```bash
pnpm format && pnpm lint && pnpm test
```

CI runs the same checks (`cargo fmt --check`, `clippy -D warnings`, Rust
tests, Prettier, ESLint, Vitest) and builds the macOS bundle.

## Ground rules

1. **Every edit is a `Command`.** New editing features add a variant to
   `dotify_core::Command`, compile it to reversible `Op`s, and get tests for
   behaviour *and* undo/redo. Never mutate a `Document` from the UI, CLI or
   future MCP server directly.
2. **Determinism.** Commands must produce identical results on every
   platform: integer math, no randomness, no time-dependence.
3. **Pixel accuracy.** RGBA8 straight alpha, nearest-neighbour only, never
   enable image smoothing.
4. **Keep the core pure.** `dotify-core` must not depend on Tauri, async
   runtimes or UI code.
5. **Keep pointer input out of React.** Canvas interaction belongs in
   `apps/desktop/src/canvas`; React components subscribe to narrow
   Zustand selectors.
6. **Mirror algorithms carefully.** If you change `raster.rs` or
   `composite.rs`, update `raster.ts`/`color.ts` and both test suites.
7. **Format changes** bump `FORMAT_VERSION`, add a migration in
   `format/migrate.rs`, and update `docs/FORMAT.md`.
8. **Types across IPC.** When Rust types exchanged with the UI change,
   update `apps/desktop/src/engine/types.ts`.

## Commit style

Small, focused commits with an imperative subject line
(`Add ellipse tool`, `Fix flood fill on locked layers`).
