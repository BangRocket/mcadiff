---
name: codebase-architecture
description: End-to-end walk of the one-walk-two-sinks invariant, all key file locations, and the IDiffSink event contract (namespace is McaGit as of 2026-06-06 rename from McaDiff)
metadata:
  type: project
---

## STATUS: .NET-era, file paths no longer exist (updated 2026-07-01)

The project has since been fully ported to Rust and the .NET tree was deleted (see
`user.md`). The *event contract and design intent* described below is still believed
accurate in spirit — the Rust port was validated against this C# implementation during the
port — but every `.cs` file path is stale. Verified-current Rust equivalents (file paths
confirmed by direct inspection, not just inference):
- `NbtComparer.cs` walk → `crates/diff/src/comparer.rs`
- `NbtChangeSink.cs` → `crates/diff/src/change.rs`
- `PatchOpSink.cs` (in Patch/) → `crates/patch/src/op_sink.rs`
- `PatchExtractor.cs` → `crates/patch/src/extract.rs`
- `PatchApplier.cs` → `crates/patch/src/apply.rs`
- `PatchModels.cs` → `crates/patch/src/model.rs`
- `NbtIdentity.cs` → `crates/nbt/src/identity.rs`
- `NbtPath.cs` → `crates/nbt/src/path.rs` (confirmed: flat iterative segment scan, not
  recursive-descent — see [[untrusted-recursion-guards]])
- `NbtJson.cs` → `crates/nbt/src/json.rs` (`to_json`/`from_json`)
- `NbtCanonical.cs` → `crates/nbt/src/canonical.rs`
- Binary read/write (not present as a distinct C# concept, fNbt handled it) →
  `crates/nbt/src/read.rs` / `crates/nbt/src/write.rs`, wrapping `valence_nbt` 0.8.0
- `WorldDiffer.cs` → `crates/diff/src/world.rs`

**Not yet re-verified this session**: whether the exact five-event contract (Added/Removed/
Modified/TypeChanged/ArrayChanged) and the "Sink Parity Status" table below still hold
symmetrically in the Rust `comparer.rs`/`change.rs`/`op_sink.rs` trio — the 2026-07-01 review
that added this note only touched `crates/nbt/{read,json}.rs` and `crates/patch/model.rs`
(tests + doc comments, zero comparer/sink changes), so parity was untouched by construction,
not re-audited from scratch. Next time `crates/diff` or `crates/patch/src/{op_sink,apply}.rs`
actually changes, redo a full parity pass and replace this whole file with a Rust-native
version rather than patching around the C# one below.

## The Load-Bearing Invariant

`NbtComparer.Walk(a, b, sink)` drives a recursive tree walk. It emits exactly five events on IDiffSink:
- `Added(path, value)` — key/element in B only; whole subtree passed
- `Removed(path, value)` — key/element in A only; whole subtree passed
- `Modified(path, a, b)` — scalar leaf changed (same type); called only for non-compound, non-list, non-array tags
- `TypeChanged(path, a, b)` — same key, different tag type; walk stops (no recursion into children)
- `ArrayChanged(path, a, b)` — ByteArray/IntArray/LongArray differs; whole array passed

Two sinks consume these events:
1. `NbtChangeSink` (display) — flattens Added/Removed subtrees to one row per leaf, summarizes arrays
2. `PatchOpSink` (patch) — emits one PatchOp per event; added/removed store whole subtree via NbtJson

## Namespace Note (updated 2026-06-06)

All source files were renamed from `McaDiff.*` to `McaGit.*` (namespace + directory) in the `chore/namespace-mcagit` branch. The move was a mechanical 1:1 token swap — zero semantic changes. AssemblyName remains `mcagit`. All paths below reference the new `src/McaGit/` tree.

## Key File Locations

### Diff/
- `IDiffSink.cs` — the five-method event interface
- `NbtComparer.cs` — the recursive walk; `Walk()` is the public entry point; `Compare()` is convenience wrapper for display
- `NbtChangeSink.cs` — display sink; flattens subtrees; has ExpandArrays logic
- `PatchOpSink.cs` — patch sink; one PatchOp per event, lossless NbtJson encoding
- `ListMatcher.cs` — derives stable keys for list elements (delegates to NbtIdentity)
- `DiffModels.cs` — WorldUnit, ChunkDiff, FileDiff, WorldDiff, DiffRunOptions record types
- `NbtChange.cs` — NbtChange record + ChangeKind enum + NbtDiffOptions
- `ValueRepr.cs` — human-readable string forms; ScalarEquals used by comparer
- `WorldDiffer.cs` — top-level orchestration; parallelizes file diffing

### Nbt/
- `NbtIdentity.cs` — KeyOf(NbtCompound): priority order: xyz coords → UUID IntArray → UUIDMost/Least → Slot byte → id string
- `NbtPath.cs` — parses dotted/bracketed paths; Get/Set/TerminalName; identity resolution via NbtIdentity.KeyOf
- `NbtJson.cs` — lossless NBT↔JSON; type-tagged single-key objects; longs/long-arrays as strings
- `NbtEquality.cs` — recursive DeepEquals; used by patch 3-way guard; float uses .Equals() (NaN==NaN)
- `NbtCanonical.cs` — deterministic binary serialization for repo object hashing; sorts compound keys recursively

### Patch/
- `PatchModels.cs` — PatchOp, ChunkPatch, PatchFileEntry, WorldPatch; JSON serialization options
- `PatchOpSink.cs` — IDiffSink→PatchOp; TypeChanged and Modified both emit Base+Value (correct parity)
- `PatchExtractor.cs` — builds WorldPatch; reuses NbtComparer.Walk via PatchOpSink
- `PatchApplier.cs` — applies WorldPatch; 3-way guard (NbtEquality.DeepEquals); supports --reverse/--force/--dry-run

## Sink Parity Status (as of 2026-06-03 audit)

TypeChanged: both sinks emit Base+Value — PARITY HOLDS
Modified: both sinks emit Base+Value — PARITY HOLDS
ArrayChanged: PatchOpSink emits whole array; NbtChangeSink summarizes or expands — PARITY HOLDS (by design, different representation)
Added: both handle whole subtree — PARITY HOLDS
Removed: both handle whole subtree — PARITY HOLDS

Key asymmetry that is BY DESIGN: NbtChangeSink flattens Added/Removed subtrees to leaf rows; PatchOpSink stores the whole subtree. This is intentional and correct — the patch doesn't need to address each leaf, it replaces the whole subtree atomically.

## Versioning

WorldPatch has `Version: 1`. No migration logic exists. Identity changes or path format changes silently break existing .mcapatch files.
