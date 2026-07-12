# Current Pipeline

Documentation role: working note. This file prevents duplicated in-flight work. It is not a roadmap; `docs/doria-end-to-end-plan.md` owns the roadmap.

## Recently merged

- PR #69: Stage 12 reusable CFG/dataflow analysis, recursion and mutual recursion, `doria-rt`, abort-only panic with Doria stack traces, and exact stdout/stderr/status parity.
- PR #70: Stage 13 fixed-width integers, operators, contextual literals, checked conversions, scalar-width ABI coverage, and durable panic parity.
- PR #71: Stage 14 IEEE floats, runtime bool values, explicit default numeric conversions, shared scalar MIR, and durable interpreter/Cranelift parity.
- PR #72: Stage 15 LLVM release backend over shared validated MIR and triple differential parity.

## Active

- Stage 16 runtime strings and canonical display conversion are complete on `feature/stage-16-runtime-strings-display` after the branch validation gates pass.
- Native remains one target: direct compile/run uses the Cranelift fast profile, while `--release` selects LLVM 18 over the same validated typed MIR.
- Immutable UTF-8 strings are Copy source values backed by private refcounted buffers. Runtime locals, rebinding, parameters, returns, calls, concatenation, byte comparison, primitive display, interpolation, echo, and panic messages share MIR and `doria-rt`.
- The durable manifest compares exact interpreter, Cranelift, and LLVM stdout, stderr, and status, including panic and Stage 16 string fixtures.

## Next

- Stage 17: `std::io` v0 and formatted I/O.

## Do not duplicate

- PR #69 Stage 12 CFG/dataflow, recursion, runtime, panic, and durable parity work.
- PR #70 Stage 13 integer-model, operator, conversion, and parity work.
- ROADMAP-style planning outside the end-to-end plan.

## Deferred

- Full arbitrary-expression interpolation and `Displayable` until Stage 18.
- `Bytes` until Stage 23.
