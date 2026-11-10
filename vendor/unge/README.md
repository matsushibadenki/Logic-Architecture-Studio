# Vendored UNGE rendering subset

`unge-core`, `unge-interaction`, `unge-render` source snapshot, supplied by the project owner on 2026-11-09. Original checkout HEAD: `8a384911d959c4bc6b9bb973ae380878b28b48e6`.

Local integration: `../../crates/renderer`. See the repository's `THIRD_PARTY_NOTICES.md`.

The subset's root Cargo.toml is supplied by Logic Architecture Studio to make its original tests runnable independently. The main application uses its own Cargo.lock.

Local rendering changes: overview labels stay visible to zoom 0.35, use minimum screen-space title/formula sizes and higher-contrast captions; fine port labels appear at zoom 0.85.

Retained-render integration: SceneIndex caches per-node geometry/text and cable tessellation with view/presentation invalidation and reuse counters. GPU instance buffers upload changed ranges only; new buffers receive a full upload. The host applies SceneChanges through its retained derived projection.
