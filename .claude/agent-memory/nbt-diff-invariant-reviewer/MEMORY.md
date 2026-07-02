# NBT Diff Invariant Reviewer — Memory Index

- [Codebase Architecture](arch.md) — one-walk-two-sinks invariant + event contract; C#-era, now annotated with verified Rust file-path equivalents (2026-07-01)
- [Untrusted-Input Recursion Guards](recursion-guards.md) — valence_nbt MAX_DEPTH=512 (binary) + serde_json 128-cap (.mcapatch JSON): exact locations, inheritance chain, pinning tests
- [Known Bugs and Risk Areas](bugs.md) — latent bugs/drift risks from the 2026-06-03 C#-era audit; some marked FIXED in Rust port, not fully re-audited
- [Test Coverage Map](tests.md) — C# + Rust test inventory; updated 2026-07-01 with recursion-guard pinning tests
- [NbtJson Round-Trip Notes](nbtjson.md) — lossiness edge cases; Rust float/double NaN fix noted
- [NbtIdentity and Patch Path Stability](identity.md) — C#-era identity key priority order, backward-compat risks (not yet re-verified against crates/nbt/src/identity.rs)
- [User and Feedback](user.md) — collaboration preferences; project is Rust-primary, not C#/.NET (corrected 2026-07-01)
