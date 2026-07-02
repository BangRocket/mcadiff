---
name: user-profile
description: User profile and preferences for collaboration on the mcagit project
metadata:
  type: user
---

The user is the primary author of mcagit, a semantic diff/patch/version-control tool for
Minecraft world files. It began as a C#/.NET proof-of-concept; as of the audit that produced
this memory directory, the project completed a full port to Rust (six-crate cargo workspace,
binary `mcagit`) and the Rust implementation is now the sole, primary version — the .NET tree
was deleted. When citing file paths, use the Rust crate layout (`crates/nbt`, `crates/diff`,
`crates/patch`, `crates/repo`, `crates/anvil`, `crates/cli`), not the old `src/McaGit/*.cs`
paths recorded in older entries in this memory directory (kept for historical bug/design
context, but the file names no longer exist on disk). They work at a high technical level —
the project has sophisticated git-like versioning, bidirectional patch application, and
careful attention to NBT format correctness.

They requested an adversarial + constructive full-subsystem audit (not just a PR review), filed as a self-contained GitHub issue in GFM. This implies:
- They want depth and specificity, not surface-level notes.
- They value concrete fix directions (code sketches) over vague advice.
- They want both bugs AND what is genuinely well-built called out.
- The report should be complete and stand alone as a GitHub issue — no conversational framing.
- Do not use emojis.
- Do not add trailing summaries or recap what was done.
