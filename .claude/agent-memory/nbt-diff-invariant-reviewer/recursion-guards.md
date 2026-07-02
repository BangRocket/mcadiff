---
name: untrusted-recursion-guards
description: The two recursion-depth guards protecting untrusted-input parsing (binary NBT, .mcapatch JSON), their exact source locations/values, and which downstream recursive walks inherit each bound. Verified 2026-07-01 against crates/nbt/src/{read,json,path,write,canonical}.rs and crates/patch/src/{model,apply}.rs.
metadata:
  type: project
---

## The two guarded entry points (Rust crates)

1. **Binary NBT decode** — `crates/nbt/src/read.rs::read()` is the *sole* call site of
   `valence_nbt::from_binary` in the whole workspace (confirmed: only `crates/nbt/Cargo.toml`
   depends on `valence_nbt` at all — `grep -rn valence_nbt:: crates/` hits only read.rs,
   conv.rs, write.rs, and only read.rs calls the decode entry point). valence_nbt 0.8.0
   (`~/.cargo/registry/.../valence_nbt-0.8.0/src/binary/decode.rs:38-39`) hard-codes
   `const MAX_DEPTH: usize = 512`; `check_depth` (decode.rs:49-56) is invoked for every
   nested `Tag::List` and `Tag::Compound` and errors with `"reached maximum recursion depth"`
   (contains "recursion") once depth reaches 512. This is the only thing standing between a
   hostile region/chunk file and a stack overflow.
2. **`.mcapatch` JSON parse** — `WorldPatch::from_json` (crates/patch/src/model.rs) calls
   `serde_json::from_str` directly. serde_json 1.0.150's `Deserializer` defaults
   `remaining_depth: 128` (de.rs:63) and decrements it on every nested compound/array via the
   `check_recursion!` macro (de.rs:1372-1385), erroring with `ErrorCode::RecursionLimitExceeded`
   → Display text `"recursion limit exceeded"` (error.rs:384, contains "recursion"). This applies
   whether deserializing into a typed struct or into `serde_json::Value` — same code path.
   Confirmed no crate in the workspace enables the `unbounded_depth` feature or calls
   `.disable_recursion_limit()` (`grep -rn unbounded_depth\|disable_recursion_limit` over
   `crates/` and all `Cargo.toml` is empty).

## Chain from `.mcapatch` file to NBT tree (why json::from_json's depth is bounded)

`crates/patch/src/apply.rs:10` imports `mca_nbt::from_json` and calls it (line ~177-178) on
`PatchOp.base` / `PatchOp.value`, which are typed `Option<serde_json::Value>` — populated by
the same `serde_json::from_str` call in `WorldPatch::from_json` above. Because that parse is
already capped at ~128 levels, the `serde_json::Value` tree `mca_nbt::json::from_json`
recursively walks can never be deeper than ~128, regardless of `from_json`'s own lack of an
explicit counter. The doc comment on `crates/nbt/src/json.rs::from_json` states this
inherited-bound argument; verified accurate.

## What's NOT independently recursion-guarded, and why that's fine

- `crates/nbt/src/write.rs` (`valence_nbt::to_binary`) and `crates/nbt/src/canonical.rs`
  (`canonical_bytes`) are recursive over `NbtValue` with no depth counter of their own. Safe
  in practice because every `NbtValue` tree that reaches them originates from one of the two
  guarded entry points above (binary read ≤512, or patch JSON parse ≤~128) — there is no third
  constructor of untrusted `NbtValue` trees in the codebase (confirmed no SNBT parser is used
  anywhere: `grep -rln snbt` over `crates/` is empty, despite valence_nbt itself shipping one
  with its own independent MAX_DEPTH=512 in `src/snbt.rs`).
- `crates/diff/src/comparer.rs`'s walk operates only on `NbtValue` trees sourced from
  `mca_nbt::read()` (two chunk files being diffed), so it inherits the 512 bound too.
- `crates/nbt/src/path.rs::NbtPath::parse` is a **flat iterative loop** (byte-index scan, no
  recursion) — a hostile/long path string cannot stack-overflow it regardless of depth-guard
  status. Not a gap; just a different (non-recursive) shape, so no pinning test is needed for
  the depth angle. A very long path is still a distinct (unaddressed) memory/DoS surface, but
  that's unrelated to stack recursion.
- `crates/repo/src/manifest.rs` (`Manifest`/`CommitObject`/`TagObject::from_json`) also goes
  through plain `serde_json::from_str` with the same default 128 cap — same protection,
  already safe, but as of 2026-07-01 has no pinning test analogous to the new patch/model.rs
  one. Low priority (repo objects are additionally hash-verified on the wire per CLAUDE.md's
  trust-boundary notes) but worth a matching test if `crates/repo` guards are ever audited.

## Pinning tests that lock these in (added 2026-07-01)

- `crates/nbt/src/read.rs::tests::absurdly_nested_input_errors_instead_of_overflowing` — 600
  nested compounds, asserts error message contains "recursion". Builds bytes inline
  (`nested_compounds(depth)` helper: `[10,0,0]` root + repeated `[10,0,1,b'a']` children + N+1
  zero End bytes via `Vec::resize`) — no binary fixture, matches house style.
- `crates/nbt/src/read.rs::tests::deep_but_sane_nesting_parses_and_canonicalizes` — 100 levels
  parses, converts, and produces non-empty canonical bytes (guard doesn't reject sane depth).
- `crates/patch/src/model.rs::tests::absurdly_nested_patch_json_errors` — 200-deep JSON object
  embedded as a `PatchOp.value`, asserts error contains "recursion".
- All three run green (`cargo test -p mca-nbt --lib`, `cargo test -p mca-patch --lib`), and
  `cargo fmt --all -- --check` / `cargo clippy -p mca-nbt -p mca-patch --all-targets -- -D
  warnings` are clean on this diff (2026-07-01).

Cross-reference [[codebase-architecture]] for where these files fit in the crate layout, and
[[test-coverage-map]] for the full test inventory.
