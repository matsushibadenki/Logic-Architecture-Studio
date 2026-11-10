# Third-party notices

## UNGE — Universal Node Graph Engine

This project vendors `unge-core`, `unge-interaction` and `unge-render` from the user-provided UNGE checkout. The source checkout reported commit `8a384911d959c4bc6b9bb973ae380878b28b48e6` on 2026-11-09; the copied files are a snapshot of that checkout's working tree.

Copyright (c) 2026 UNGE contributors. Used under the MIT license. The complete notices are retained in `vendor/unge/LICENSE-MIT` and `vendor/unge/LICENSE-APACHE`.

The native renderer uses UNGE directly via path dependencies. The WebView node presentation and port-anchor calculation are adapted from its `NODE_APPEARANCE.md`, `theme.rs`, and `spatial.rs`. The GPU test readback helper is adapted from its `examples/gallery.rs`.

The original UNGE checkout is unchanged. This repository has no runtime dependency on its original absolute path. The adapter in `crates/renderer` owns an immutable rendering projection of the canonical circuit; UNGE does not own a second editable circuit.

The copied subset intentionally excludes ACX, execution, Tauri host, and generated bindings. Those UNGE features are not represented as features of this application.
