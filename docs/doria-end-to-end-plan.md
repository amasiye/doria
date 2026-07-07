# Doria End-to-End Development Plan

**Document ID:** docs/doria-end-to-end-plan.md
**Status:** Proposed master plan for Doria v0.1 → v1.0
**Audience:** The implementing agent (Codex) and the language designer
**Supersedes nothing.** This plan extends SPEC.md and the accepted decisions in `docs/decisions/`. Where this plan resolves something SPEC.md marked "future work", this plan is the accepted direction once the language designer approves the decision list in Section 1.

---

## 0. How to use this document

This is the single authoritative execution plan. It exists so that implementation can proceed **without design back-and-forth**. It does three things:

1. **Resolves every open language-design fork** in SPEC.md with a concrete accepted default, each traceable to a numbered decision record (Section 12 lists the records to author).
2. **Defines the full compiler, runtime, standard library, tooling, and PHP-interop architecture** from the current Stage 10 slice to v1.0.
3. **Sequences the work into phases and stages** with explicit scope, out-of-scope lists, and acceptance criteria, in the same incremental style as Stages 1–10.

**Rules of engagement for the implementing agent:**

- Implement stages strictly in order within a phase. Phases may not be reordered without designer approval.
- Every stage ships with: a decision record (if it introduces semantics), integration tests in `crates/doriac/tests`, updated `SPEC.md` and `README.md` sections, updated editor token guardrails when vocabulary changes, and example programs in `examples/`.
- The "stop and ask" rule from SPEC.md §1.1 still applies, but **only for forks not answered by this document**. If this document answers it, implement it as written. If this document and SPEC.md conflict, this document wins for future-work items and SPEC.md wins for already-implemented behavior; flag the conflict in the stage's decision record either way.
- Native-first correctness policy is unchanged: Doria semantics → Doria IR → backend lowering. The PHP backend never defines semantics.
- Temporary backend limitations remain unsupported-feature diagnostics, never redefinitions of the language.

---

## 1. Decisions this plan makes — designer review checklist

These are the load-bearing choices. Each becomes a decision record before its first implementing stage lands. Approve, amend, or veto these first; everything downstream is consistent with them.

| # | Decision | Accepted default in this plan |
|---|----------|-------------------------------|
| D1 | Memory model | Automatic reference counting (ARC) for class instances with deterministic `__destruct`; value semantics with copy-on-write for collections; immutable value-semantic strings; `weak` references to break cycles; **no tracing GC, no borrow checker, no lifetime annotations in surface syntax** |
| D2 | Aliasing/mutation safety | readonly/writable is the aliasing model. Law of exclusivity: a `writable` access to a value excludes overlapping access for the duration of the operation. Enforced statically where provable, dynamically in debug profile where not |
| D3 | Value vs reference | Primitives, `string`, ranges, enums, and collections are value types. Classes are reference types. No user-defined `struct` in v1.0 (revisit for v1.x if engine profiling demands it) |
| D4 | Integer overflow | Arithmetic overflow panics in both dev and release profiles. Explicit `Int::wrappingAdd(...)`, `Int::saturatingAdd(...)`, `Int::checkedAdd(...)` for other behavior. A `declare` key may later relax this per-module for engine hot paths |
| D5 | Nullability | `?Type` optional types (PHP spelling), `??` null coalescing, `?->` null-safe access. `null` is not assignable to non-`?` types. No implicit truthiness |
| D6 | Enums | PHP 8.1-shaped `enum` declarations extended with payload cases (tagged unions): `case Some(int $value);`. This is Doria's sum type |
| D7 | Pattern matching | `match` is expression-position, exhaustiveness-checked over enums/bools/finite domains, PHP 8 `match` spelling extended with payload destructuring. `when` is the value-returning conditional chain per decision 0009 |
| D8 | Errors | Checked `throw`/`throws` with PHP-shaped `try`/`catch`/`finally`. Errors are class instances implementing the built-in `interface Error`. `Result<T, E>` stays out of the surface model per decision 0035 |
| D9 | Generics | Monomorphized generics for functions, classes, interfaces, traits. Constraint spelling: `<T implements Comparable>`. No runtime generic reflection in v1.0 |
| D10 | Closures | PHP-shaped anonymous `function (...) use (...) { }` with explicit capture list, plus auto-capturing arrow functions `fn($x) => ...` with readonly by-value capture. `use (writable $x)` captures by shared mutable cell only for local, non-escaping closures; escaping mutable capture is a compile error in v1.0 |
| D11 | Concurrency | Structured concurrency with `async function` / `await` / task groups; data-race freedom enforced through readonly-by-default plus a `Shareable` marker interface checked at spawn boundaries. Detailed design gated behind its own decision record in Phase H |
| D12 | Unsafe & FFI | `unsafe { }` blocks gate raw pointers (`Ptr<T>`, `MutPtr<T>`), foreign calls, and manual memory. `extern` declarations bind C ABI symbols. Everything outside `unsafe` keeps full safety guarantees |
| D13 | PHP bridge (the strategic pillar) | Three interop products: (a) existing Doria→PHP compat backend, (b) `doriac migrate php`, and **(c) `doriac build --php-lib`: compile Doria libraries to a C-ABI shared library plus generated PHP FFI stub classes, so PHP applications call native Doria directly.** (c) is the "powerful backends for PHP" product and gets its own phase |
| D14 | Division/modulo | `/` on `int` is truncating integer division; `%` is remainder with sign of dividend (C/PHP `intdiv`-consistent). Division/modulo by zero panics. `float` division follows IEEE 754 |
| D15 | Numeric widening | No implicit conversions anywhere, including int→float. Explicit `Int::toFloat($x)`, `Float::toInt($x)` (truncating, panics on NaN/out-of-range), and fixed-width conversions via `Int32::from($x)` (panics on overflow) / `Int32::tryFrom($x)` (nullable) |
| D16 | String encoding | `string` is immutable UTF-8. Byte-level work uses `Bytes` (mutable, value-semantic, COW). Indexing a `string` by integer is not allowed; iteration yields grapheme clusters via `$s->chars` is deferred, `$s->bytes` ships first |
| D17 | Inheritance model | Single class inheritance, multiple interface implementation, trait composition via `uses` with explicit conflict resolution (`insteadof`/`as` PHP spelling accepted). Methods are non-virtual by default; `open function` opts into overriding; `override function` required at override sites |
| D18 | Standard entry runtime | Every native binary links `doria-rt` (Rust-implemented runtime library): allocator, ARC ops, string/collection intrinsics, panic machinery, stdout/stderr. `doria-rt` is an internal ABI, not public, until v1.0 |

Everything below elaborates these decisions into implementable specifications.

---

## 2. Vision, positioning, and end products

Doria is a statically checked, natively compiled systems language with PHP-shaped syntax and Rust-grade safety defaults, minus Rust's lifetime/borrow surface language. The strategic products it must eventually support, in priority order:

1. **A native systems language** producing standalone executables (already the accepted direction).
2. **The PHP power-backend story**: when a PHP application hits performance or capability limits, teams write the hot module in Doria and call it from PHP with near-zero friction, because the syntax is already familiar and the bridge is first-class (D13c). This is Doria's unique adoption wedge — no other native language offers PHP developers syntax continuity plus a generated, type-checked FFI bridge.
3. **A game engine written in Doria**, which drives requirements for: deterministic destruction (ARC, no GC pauses), fixed-width numerics, floats and SIMD, unsafe/FFI for graphics/audio/input APIs, allocator control, and predictable value-type collections.
4. **A UI framework** integrating with PHP web backends, which drives requirements for: attributes-as-metadata, property hooks, closures, enums, pattern matching, and async.

The plan sequences language work so that requirement sets 2–4 unlock in that order.

---

## 3. Memory model and safety (D1–D3, D12)

This is the foundational design SPEC.md has not yet stated. It takes Rust's *outcomes* — no use-after-free, no data races, no null surprises, deterministic cleanup — and delivers them through mechanisms that need no lifetime annotations or borrow-checker vocabulary.

### 3.1 Value types and reference types

- **Value types**: `int` family, `uint` family, `float` family, `bool`, `string`, `Bytes`, ranges, enums (including payload enums), and the collection family `List<T>`, `Dictionary<K, V>`, `Set<T>`.
- **Reference types**: class instances, closures, `resource`.

Assigning or passing a value type copies it *logically*. Strings and collections are heap-backed but **copy-on-write**: assignment copies a pointer and bumps a refcount; the first `writable` mutation through a handle with refcount > 1 clones the buffer. This matches the mental model PHP developers already have — PHP arrays are literally copy-on-write value types — while giving Rust-grade freedom from aliasing bugs. No spooky action at a distance through a `List<T>` is possible.

Assigning or passing a class instance copies a reference; both names see the same object, exactly as in PHP.

### 3.2 Class instance lifetime: ARC with deterministic destruction

- Every class instance carries a strong reference count managed entirely by the compiler (retain/release inserted during MIR lowering; elided by optimization when provably redundant).
- When the strong count reaches zero, `__destruct` runs immediately and deterministically, then memory is freed. This gives RAII: files, GPU buffers, locks, and sockets close at scope exit with zero GC pauses — a hard requirement for the game engine.
- **Cycles**: strong reference cycles leak by design (documented), and Doria provides `weak` property/binding modifier producing a `?T`-reading weak reference that becomes `null` after the target is destroyed:

```doria
class Node
{
    writable ?Node $next;
    weak writable ?Node $parent;
}
```

- `weak` implies the declared type is nullable; declaring `weak` on a non-`?` type is a compile error.
- Dev-profile builds may include an opt-in cycle detector diagnostic tool (`doriac run --detect-cycles`) in a later stage; it is a debugging aid, never a collector.

### 3.3 Aliasing and the law of exclusivity (D2)

readonly/writable is not just documentation — it is the aliasing model:

- A readonly binding/parameter/`$this` guarantees the callee cannot mutate through it. Because collections and strings are value types, a readonly `List<int>` parameter can never change under the caller either.
- **Law of exclusivity**: two overlapping accesses to the same memory where at least one is `writable` are illegal within a single operation. The classic hazard is a writable method receiving (an alias of) its own object as a readonly argument. Enforcement mirrors Swift: statically rejected where the checker can prove overlap; dev-profile builds insert cheap dynamic exclusivity checks for the unprovable remainder; release builds may omit the dynamic checks under a `declare` key once the policy stage lands.
- There are no lifetime parameters, no `&`/`&mut` spellings, and no borrow diagnostics vocabulary anywhere in the surface language. The words are `readonly` (implicit), `writable`, and `weak`.

### 3.4 Unsafe and FFI (D12)

For engine internals and C interop:

```doria
extern "C" {
    function malloc(uint64 $size): Ptr<void>;
    function free(Ptr<void> $ptr): void;
}

function fastCopy(writable Bytes $dst, Bytes $src): void
{
    unsafe {
        // raw pointer work permitted only here
    }
}
```

- `unsafe { }` is the only context where `Ptr<T>` / `MutPtr<T>` may be dereferenced, `extern` functions called, and refcounts manually manipulated (`Rc::retain`, `Rc::release` intrinsics).
- `extern "C"` blocks declare foreign symbols; parameter/return types restricted to FFI-safe types (fixed-width numerics, `Ptr<T>`, `Bool8` later).
- An `unsafe function` spelling marks a whole function as requiring an unsafe context to call.
- `declare` keys will later govern per-module unsafe policy (deny/allow), per decision 0028's directive direction.

### 3.5 Panics

A panic is a fatal runtime error, distinct from checked `throw`/`throws` per decision 0035: arithmetic overflow, division by zero, out-of-bounds indexing, failed `Float::toInt`, explicit `panic("message")`. Default behavior: print message + Doria stack trace to stderr, run no destructors beyond what the unwinding decision allows, exit with status 101. **v1.0 panic policy is abort-only (no unwinding, no catching panics).** This keeps codegen simple and honest; checked errors are the recoverable path.

---

## 4. Type system completion (D4–D9, D14–D16)

### 4.1 Numerics

- Full fixed-width family per decision 0016 becomes real compiler types: `int8/16/32/64`, `uint8/16/32/64`, `float32/64`; `int` = `int64`, `float` = `float64`.
- Literals: `42` is `int` unless the expected type in context is another integer type and the literal fits (contextual typing, checked at compile time; `int8 $x = 200;` is a compile error). `4.2` is `float` with the same contextual rule for `float32`. Suffixed literal spellings are **not** added; contextual typing plus `Int32::from(...)` covers the need.
- Operators complete: `/`, `%` (D14), bit shifts `<<` `>>` (arithmetic right shift on signed; shifting by ≥ bit-width panics), bitwise `& | ^ ~` on all integer types.
- No implicit widening (D15). Mixed-type arithmetic (`int + int32`) is a compile error; convert explicitly.

### 4.2 Nullable types (D5)

```doria
?Person $found = $repo->findById($id);

let $name = $found?->name ?? "anonymous";

if ($found != null) {
    echo $found->name;   // flow-narrowed to Person in this block
}
```

- `?T` is `T` or `null`. `null` literal has type `null` and is assignable only to `?T` and `mixed`.
- Flow-sensitive narrowing: `!= null` / `== null` comparisons and `match` narrow `?T` to `T` inside the guarded region. This is the first path-sensitive analysis and lands as its own stage.
- Representation: `?T` for reference types uses null pointers (zero cost); for value types uses a discriminant word (niche optimization is a backend improvement later).
- `mixed` remains the dynamic escape hatch for PHP-interop shapes; narrowing `mixed` requires `match` or explicit `is` checks (`$x is string`) introduced in the same stage as narrowing.

### 4.3 Enums and payload enums (D6)

```doria
enum Status
{
    case Draft;
    case Published;
    case Archived;
}

enum Shape
{
    case Circle(float $radius);
    case Rect(float $width, float $height);
}
```

- Backed enums (`enum Level: int { case Low = 1; ... }`) supported with PHP spelling.
- Payload cases make `enum` Doria's tagged union: value-semantic, monomorphized with generics later (`enum Option<T> { case None; case Some(T $value); }` ships as a stdlib type once generic enums land).
- Enum values compare with `==` by case + payload equality.

### 4.4 match and when (D7)

`match` is a value-returning expression with mandatory exhaustiveness over closed domains:

```doria
let $area = match ($shape) {
    Shape::Circle($r) => 3.14159265 * $r * $r,
    Shape::Rect($w, $h) => $w * $h,
};

let $label = match (true) {
    $n < 0 => "negative",
    $n == 0 => "zero",
    default => "positive",
};
```

- Arms: enum case patterns with payload destructuring, literal patterns, `null` pattern, `default`. Guards (`Shape::Circle($r) if $r > 1.0 =>`) are a fast-follow stage.
- Non-exhaustive `match` over an enum or `bool` without `default` is a compile error.
- `when` (decision 0009) is the value-returning conditional chain; it lands after `match` since `match (true)` covers most needs, and its grammar gets its own decision record in Phase E.

### 4.5 Generics (D9)

```doria
function first<T>(List<T> $items): ?T
{
    // ...
}

class Stack<T>
{
    internal writable List<T> $items = [];

    writable function push(T $item): void { /* ... */ }
    writable function pop(): ?T { /* ... */ }
}

function max<T implements Comparable<T>>(T $a, T $b): T
{
    return match (true) {
        $a->compareTo($b) >= 0 => $a,
        default => $b,
    };
}
```

- Monomorphization at MIR level: each concrete instantiation generates specialized code (Rust model — zero-cost, no boxing). Compile-time cost is accepted; the dev backend (Cranelift) keeps iteration fast.
- Constraint spelling `T implements Interface` keeps Doria's own vocabulary; multiple constraints with `+`? **No** — spelling is `T implements A, B` inside the angle brackets, comma-separated, matching `implements` lists.
- Generic type inference at call sites from argument types; explicit turbofish-style spelling is **not** adopted — where inference fails, bind through a typed declaration.
- Collections `List<T>`, `Dictionary<K, V>`, `Set<T>` become real generic types in the compiler (they already have checked arity) backed by runtime intrinsics, then by stdlib generic implementations as self-hosting matures.

### 4.6 Strings and Bytes (D16)

- `string`: immutable, UTF-8, value-semantic (internally refcounted buffer; immutability makes COW trivial). `$s->length` is byte length; `$s->isEmpty`, `$s->bytes` accessor returning `Bytes` view (copy in v1.0).
- `Bytes`: mutable COW byte buffer for binary work, file I/O, network buffers, engine assets.
- Concatenation `.` stays string-only (already accepted). Interpolation grows to full expressions in braces `{...}` in its own stage; display conversion is governed by a built-in `interface Displayable { function toString(): string; }` — interpolating a non-Displayable class remains a compile error, resolving SPEC §7's open display-conversion question.
- Ordered comparison of strings (`<`, `<=`, ...) is byte-lexicographic; locale-aware collation is stdlib territory, not operators.

---

## 5. Error handling: checked throw/throws (D8)

Full semantics for decision 0035's accepted direction:

```doria
class NotFoundError implements Error
{
    function __construct(string $message)
    {
    }
}

function loadUser(string $id): User throws NotFoundError, StorageError
{
    let $row = $db->find($id);      // $db->find declares `throws StorageError`
    if ($row == null) {
        throw new NotFoundError("no user {$id}");
    }
    return User::fromRow($row);
}

function handler(): Response
{
    try {
        let $user = loadUser("42");
        return Response::ok($user);
    } catch (NotFoundError $e) {
        return Response::notFound($e->message);
    } catch (StorageError $e) {
        return Response::serverError($e->message);
    } finally {
        $metrics->record();
    }
}
```

Rules:

- `interface Error` is built-in with a required readonly `string $message` property requirement (property requirements on interfaces land in the same stage, scoped to this need first).
- Only class types implementing `Error` may be thrown or listed in `throws`.
- **Checked propagation**: a call to a `throws`-declared function must be (a) inside a `try` whose `catch` arms cover every declared error type (covering = the arm type is the error class or a superclass/implemented interface), or (b) inside a function whose own `throws` clause covers the uncovered remainder. `main` may declare `throws Error`; an error escaping `main` prints the error and exits with status 70.
- `catch (Error $e)` is the catch-all. Rethrow is plain `throw $e;`.
- `finally` runs on normal exit, thrown-error exit, and early `return`; it may not `return`, `throw`, `break`, or `continue` (avoids PHP/Java's swallowed-error trap; compile error).
- Lowering: `throws` functions return a hidden discriminated result in the native ABI (no unwinding — consistent with the abort-only panic policy and cheap for the engine). The PHP backend lowers to native PHP exceptions.
- `throw` is a statement in v1.0; expression-position `throw` (PHP 8 style) is a fast-follow.

Panics (Section 3.5) remain entirely separate: not declarable, not catchable.

---

## 6. OOP completion (D17)

### 6.1 Inheritance and dispatch

```doria
open class Model
{
    open function save(): void throws StorageError { /* ... */ }
    function id(): string { /* ... */ }        // not overridable
}

class Post extends Model
{
    override function save(): void throws StorageError { /* ... */ }
}
```

- Classes are **closed by default**; `open class` permits subclassing. This is the Rust/Kotlin idea (inheritance as a deliberate API) in plain spelling, and it lets the compiler devirtualize aggressively — important for engine performance.
- Methods are non-virtual by default; `open function` creates a vtable slot; `override function` is mandatory at override sites (typo-proof).
- Single inheritance; construction order: property initializers of the subclass run, then `__construct` body, which must call `parent::__construct(...)` first if the parent declares a constructor with required parameters (checked).
- `internal` members are never inherited-visible; there is still no `protected` in v1.0 (revisit only with real evidence of need).
- Upcasts implicit; downcasts via `$x is Post` narrowing and `match`; no unchecked cast spelling exists.

### 6.2 Interfaces

- Method requirements plus (from the Error work) readonly property requirements.
- Interfaces may extend multiple interfaces. Conformance is nominal via `implements`, checked at compile time.
- Default method bodies in interfaces: deferred to v1.x (traits cover reuse).

### 6.3 Traits

```doria
trait HasSlug
{
    writable string $slug = "";

    writable function refreshSlug(string $from): void
    {
        $this->slug = Slug::from($from);
    }
}

class Article
{
    uses HasSlug;
    uses Timestamps { touchedAt as internal; }
}
```

- Traits contribute properties and methods textually-by-semantics (flattened at class composition, monomorphized like generics — no runtime trait objects).
- Conflicts (two traits provide the same member) are a compile error resolved with PHP-spelled `insteadof` / `as` clauses inside the `uses` block; `as internal` may tighten surface.
- Traits may declare abstract requirements (`function render(): string;` with no body) the composing class must satisfy.

### 6.4 Property hooks

The planned escape hatch from SPEC §6, landing after classes are fully native:

```doria
class Temperature
{
    internal writable float $celsius = 0.0;

    float $fahrenheit {
        get => $this->celsius * 9.0 / 5.0 + 32.0;
        set ($value) => $this->celsius = ($value - 32.0) * 5.0 / 9.0;
    }
}
```

PHP 8.4 hook spelling; `get`-only hooks make computed readonly properties; `set` hooks require the property (or hook) to be writable-consistent.

### 6.5 Statics and constants

- `static` properties/methods with PHP spelling `ClassName::member()`; static properties follow readonly/writable rules; writable statics are per-process globals and are rejected inside `Shareable`-checked concurrency contexts later.
- `const NAME = expr;` class constants and namespace-level constants; const expressions are compile-time evaluated over literals, arithmetic, and other consts (this defines the first compile-time evaluation tier, which attributes will reuse).

---

## 7. Namespaces, source organization, closures

- Implement decision 0028 as written: `namespace App\Services;`, file-scope `use ... as ...`, string-literal include-once `include`, structured `declare`. Multi-file compilation units are the enabler stage for everything package-shaped.
- Name resolution: a compilation invocation takes a root set of files (later, Baton passes it); symbols resolve by fully qualified name; unqualified names resolve via current namespace then `use` imports; duplicate symbol definitions across files are compile errors with both spans.
- First `declare` keys (each rejected until implemented): `declare(overflow: "wrapping");` (module-local, D4 relaxation for engine hot paths), `declare(unsafe: "deny");`, `declare(exclusivity_checks: "off");` (release-profile only).
- Closures (D10):

```doria
let $double = fn($x) => $x * 2;                      // inferred, auto-capture readonly
let $adder = function (int $x): int use ($base) {    // explicit capture
    return $x + $base;
};
```

Captures are readonly by-value snapshots by default (value types copy; class references retain). `use (writable $counter)` is permitted only when the closure provably does not escape the declaring scope (not stored, not returned, not passed to `async`); otherwise compile error suggesting a class-based accumulator. Closures are reference-typed values with type spelling `function(int): int` in type position; `Callable<...>` alias is not adopted.

---

## 8. Compiler and runtime architecture plan

### 8.1 Pipeline evolution

```text
source → lexer → parser → AST
      → name resolution (namespaces, use, include)
      → semantic analysis + type checking (HIR)
      → readonly/writable + exclusivity checking
      → definite-initialization & flow analysis (narrowing, returns, ctor init)
      → Doria IR (checked, typed, desugared)
      → MIR (SSA-ish control-flow graph: retain/release insertion, drop/destruct
             placement, monomorphization, exhaustiveness lowering, panic edges)
      → backend (Cranelift dev | LLVM release | PHP compat | wasm later)
```

- The private `NativeSmokeModule` is retired in Phase A, replaced by the real MIR layer. MIR is the permanent native-oriented IR SPEC §13 anticipated. Until v1.0, MIR is not a stable format.
- Full path-sensitive control-flow analysis (returns on all paths, definite readonly-property initialization on all constructor paths, null narrowing) is one shared dataflow framework built once in Phase A and reused everywhere — it replaces the "final statement must be return" early rule.

### 8.2 Dual backend (decision 0012, made concrete)

- **Dev profile** (`doriac build`, `doriac run`): Cranelift, fast compile, dynamic exclusivity checks on, overflow checks on, debug info.
- **Release profile** (`doriac build --release`): LLVM (via `inkwell`), optimizations, overflow checks still on per D4, exclusivity dynamic checks controlled by declare policy.
- Identical Doria-visible semantics across profiles is a tested invariant: the differential test suite runs every `examples/native` program under both backends plus the interpreter and asserts identical stdout/exit status.
- **Debug/interpreter backend** (SPEC §1's listed backend) is implemented in Phase A as a direct MIR interpreter. It is the semantic oracle for differential testing and makes the test suite backend-independent — this is the single highest-leverage correctness investment in the plan.

### 8.3 doria-rt (D18)

A Rust `crates/doria-rt` static library linked into every native binary:

- Allocator (system malloc initially; pluggable arena hooks reserved for the engine later).
- ARC ops (`dr_retain`, `dr_release`, destructor dispatch), weak reference tables.
- String/Bytes/List/Dictionary/Set intrinsic implementations (COW buffers, hashing, growth).
- Panic machinery, stack trace capture, process entry glue (`dr_main` wrapping user `main`).
- stdout/stderr/stdin, basic clock, environment access — the syscall surface the stdlib wraps.

All symbols `dr_`-prefixed, internal ABI, versioned in lockstep with the compiler.

### 8.4 Diagnostics

Adopt error codes now (`D0001`-style) before the count explodes; every diagnostic carries code, span(s), message, and machine-applicable suggestion where possible; `doriac check --json` for tooling; LSP reuses the same diagnostics verbatim (already the architecture).

### 8.5 Testing strategy (all phases)

- Unit tests per compiler pass; integration tests per stage in `crates/doriac/tests` (current pattern).
- Differential suite: interpreter vs Cranelift vs LLVM on every executable example.
- UI-style diagnostic snapshot tests (expected diagnostics per fixture file) so error messages are versioned.
- The PHP backend keeps its own snapshot tests but is never the proof of semantics (unchanged policy).
- Fuzzing the lexer/parser with `cargo-fuzz` starts in Phase B (cheap, catches panics early).

---

## 9. Standard library plan

Two layers, both written in Doria as early as possible (self-hosting on-ramp):

- **core** (no I/O, always available): `Int`/`Int8`.../`Float`/`Bool`/`String` companion APIs (`Int::parse`, `Int::toFloat`, `Int::wrappingAdd`, ...), `Option`-free nullable helpers, `Comparable<T>`, `Equatable<T>`, `Hashable`, `Displayable`, `Error`, `Iterable<T>`/`Iterator<T>` (powers `foreach` over collections), range types, `math` basics.
- **std** (hosted): `io` (files, stdin/out streams), `fs`, `env`, `process`, `time`, `random`, `json` (drives enum/match/mixed ergonomics and the PHP bridge), `net` (TCP first), later `http`.

`foreach (collection as ...)` desugars to `Iterable<T>` in Phase D, making user types iterable — required for engine scene graphs and UI trees.

Stdlib API style follows SPEC §6's nouns-are-properties rule and the collection method surface gets its own decision record (List: `add`, `insertAt`, `removeAt`, `contains`, `count` property, `isEmpty` property, `map`/`filter`/`reduce` after closures land; Dictionary: `get` returning `?V`, `set`, `remove`, `has`, `keys`, `values`; Set: `add`, `remove`, `has`, `union`, `intersect`).

---

## 10. PHP interop: the three products (D13)

### 10.1 Doria → PHP compat backend (exists)
Keeps growing opportunistically for migration/debugging; never gates a language feature. Features PHP cannot express lower where practical or emit unsupported-feature diagnostics (unchanged policy).

### 10.2 PHP → Doria migration (`doriac migrate php`)
Phase I product, per SPEC §12: conservative output, diagnostics for dynamic PHP (variable variables, `eval`, magic methods, loose comparisons become explicit conversions or `mixed` + TODO diagnostics). Architecturally separate crate `crates/doria-migrate` with its own PHP parser (use `mago`/`php-parser-rs` class of dependency; do not touch the Doria parser).

### 10.3 The strategic product: `doriac build --php-lib`

Compile a Doria library to something a running PHP application calls natively:

```doria
namespace App\Native;

#[PhpExport]
class ImageResizer
{
    function resize(Bytes $input, int $width, int $height): Bytes throws ResizeError
    {
        // hot-path native code
    }
}
```

```bash
doriac build src/native --php-lib --out build/app_native
# emits: build/app_native/libapp_native.so
#        build/app_native/php/ImageResizer.php   (generated FFI stubs)
```

```php
<?php // in the existing PHP app
use App\Native\ImageResizer;              // generated stub, feels like a normal class
$resizer = new ImageResizer();
$out = $resizer->resize($bytes, 800, 600); // dispatches through FFI into native Doria
```

Design:

- Exported surface restricted to a bridgeable type set: numerics, `bool`, `string`, `Bytes`, `?T` of those, `List`/`Dictionary` of bridgeable types, and `#[PhpExport]` classes (marshaled as opaque handles owned by Doria's ARC; the PHP stub holds the handle and releases it in `__destruct`).
- `throws` errors surface as generated PHP exception classes.
- Transport: C ABI shim generated by `doriac` + PHP ≥ 8.0 `FFI` stubs first (zero build tooling required on the PHP side); a Zend-extension emission mode (`--php-ext`) is a later optimization stage for call-overhead-sensitive users.
- Threading: v1 bridge is single-threaded per PHP request (matches PHP's model); `Shareable` interactions revisited with Phase H.
- This product plus `std::json`/`net` also covers the sidecar pattern (Doria service, PHP client), but the in-process bridge is the headline.

---

## 11. Baton and developer experience

Baton lands mid-plan (Phase F), once multi-file compilation exists to orchestrate:

- `baton new <name>` (binary/lib/php-lib templates), `baton build [--release]`, `baton run`, `baton test`, `baton check`.
- Manifest `Baton.toml`: package name, version, edition placeholder, dependency table (path dependencies first; registry protocol deferred to post-1.0 — do not build a registry server in this plan).
- `baton test` defines the Doria test convention: `tests/*.doria` files whose functions marked `#[Test]` run and report (first real consumer of attributes).
- Baton drives `doriac`; it never owns semantics. LSP/editors gain workspace awareness from `Baton.toml`.

---

## 12. Decision records to author (numbering continues from 0037)

0038 memory model: ARC + COW value types (D1–D3) · 0039 exclusivity enforcement (D2) · 0040 panics & overflow policy (D4, §3.5) · 0041 division/modulo/shifts (D14) · 0042 numeric conversions (D15) · 0043 MIR + interpreter oracle (§8.1–8.2) · 0044 doria-rt ABI (D18) · 0045 runtime strings/Bytes (D16) · 0046 nullable types & narrowing (D5) · 0047 enums & payload cases (D6) · 0048 match (D7) · 0049 checked errors full semantics (D8) · 0050 generics & monomorphization (D9) · 0051 collections runtime & API surface (§9) · 0052 iteration protocol · 0053 inheritance/open/override (D17) · 0054 traits & conflict resolution · 0055 property hooks · 0056 statics & const evaluation · 0057 closures (D10) · 0058 namespaces implementation notes (elaborating 0028) · 0059 attributes & compile-time evaluation policy · 0060 Baton manifest & test convention · 0061 unsafe/FFI (D12) · 0062 php-lib bridge (D13c) · 0063 async & Shareable (D11) · 0064 when grammar · 0065 SIMD/engine intrinsics direction.

Each record follows the existing template: context, decision, alternatives considered, consequences, affected components.

---

## 13. Phased roadmap with stages and acceptance criteria

Stages continue the existing numbering. Every stage = decision record(s) + tests + docs + examples, per Section 0. "AC" = acceptance criteria.

### Phase A — Real native foundation (Stages 11–15)
Retire the smoke architecture; make the native path general.

- **Stage 11 — MIR + interpreter oracle.** Introduce MIR; port all Stage ≤10 lowering onto it; delete `NativeSmokeModule`; ship the MIR interpreter as `--target debug`; stand up the differential test harness. AC: every existing native example produces identical output under interpreter and Cranelift; no smoke-module code remains.
- **Stage 12 — General control flow.** Arbitrary/nested loops, `return` anywhere, unbounded `while`, `break`/`continue` everywhere, recursion and mutual recursion; shared dataflow framework replaces the final-statement-return rule with returns-on-all-paths. AC: recursive fibonacci, nested-loop matrix example, early-return search all run natively; loop-verification cap removed.
- **Stage 13 — Full integer family + operators.** All fixed-width types in the compiler; `/`, `%`, shifts, bitwise across widths; contextual integer literals; overflow/div-zero panics with runtime messages via doria-rt panic machinery. AC: differential tests over an arithmetic torture fixture; panic exit status 101 with message.
- **Stage 14 — Floats + bool runtime.** `float32/64` arithmetic/comparison codegen, bool as runtime value (not just conditions), `Float`/`Int` conversion companions. AC: numeric integration examples match interpreter bit-for-bit for f64 ops.
- **Stage 15 — LLVM release backend.** `--release` through LLVM over the same MIR; differential suite triples. AC: all examples identical across interpreter/Cranelift/LLVM; release binaries pass the suite.

### Phase B — Runtime strings and I/O (Stages 16–18)
- **Stage 16 — doria-rt strings.** Heap `string` (immutable, refcounted), runtime concatenation, writable string locals, string equality/ordering, full interpolation of currently-interpolable types at runtime. AC: string-building loop example; concat of function results; leak checker (Miri/valgrind CI job) clean.
- **Stage 17 — Bytes + std::io v0.** `Bytes`, stdin/stdout/stderr streams, file read/write. AC: cat-clone and line-count example programs.
- **Stage 18 — Interpolation of expressions + Displayable.** Full `{expr}` interpolation; `Displayable` interface (compiler-known); parser fuzzing job lands. AC: `echo "sum: {a() + b()}"`; interpolating a non-Displayable class is a compile error with suggestion.

### Phase C — Classes go native (Stages 19–22)
- **Stage 19 — Object layout + construction.** Native class layout, `new`, property init expressions, promoted params, ARC retain/release insertion, deterministic `__destruct`, `weak`. AC: destructor-order example; weak-parent tree example; leak CI clean.
- **Stage 20 — Methods, statics, internal.** Instance/static method codegen, `internal` enforcement in native path, class constants + const evaluation tier. AC: the SPEC §6 `Parser` class runs natively.
- **Stage 21 — Definite initialization + exclusivity.** Constructor definite-initialization on all paths (finishing SPEC §5's future-work note); static exclusivity checking + dev-profile dynamic checks. AC: fixture matrix of legal/illegal ctor flows; exclusivity violation caught dynamically in dev profile test.
- **Stage 22 — Nullable + narrowing + `is`.** D5 complete. AC: null-safe chaining example; narrowing snapshot diagnostics.

### Phase D — Collections and generics (Stages 23–26)
- **Stage 23 — Runtime collections.** COW `List/Dictionary/Set` intrinsics in doria-rt; literals, indexing (`$list[0]`, panic OOB), `foreach` over collections; value-semantics differential tests (mutate-after-copy fixtures). AC: PHP-array-intuition fixture proving no aliasing.
- **Stage 24 — Generic functions.** D9 for free functions/methods, monomorphization in MIR. AC: `first<T>` works across int/string/class lists.
- **Stage 25 — Generic classes/interfaces/traits + iteration protocol.** `Stack<T>`; `Iterable<T>`/`Iterator<T>`; `foreach` desugars to protocol. AC: user-defined iterable consumed by `foreach`.
- **Stage 26 — Collection API surface.** Decision 0051 methods incl. `map`/`filter` once Stage 30 closures exist (split: non-closure API here, closure API revisited in Stage 30). AC: stdlib written in Doria compiles via `include` (pre-Baton).

### Phase E — Enums, match, errors (Stages 27–29)
- **Stage 27 — Enums + payload cases.** D6, value-semantic layout. AC: `Shape` example native.
- **Stage 28 — match.** D7, exhaustiveness, payload destructuring, narrowing integration; guards fast-follow within the stage. AC: exhaustiveness diagnostics snapshots; `match (true)` chains.
- **Stage 29 — Checked errors end-to-end.** D8: `throws` ABI, `try/catch/finally`, `Error` interface with property requirement, `main throws`. AC: SPEC-style `loadUser` example; uncovered-error diagnostics; finally-ordering fixture.

### Phase F — Multi-file, namespaces, Baton (Stages 30–33)
- **Stage 30 — Closures.** D10 incl. escape analysis for writable captures; function types in type position; collection closure APIs unlock. AC: sort-with-comparator example; escaping-writable-capture compile error fixture.
- **Stage 31 — Namespaces/use/include/declare.** Decision 0028 implemented; multi-file compilation; first declare keys. AC: multi-file example project builds; duplicate-symbol diagnostics.
- **Stage 32 — Attributes.** `#[...]` parsing, type-checked against attribute classes, const-evaluation-tier arguments (resolving SPEC §11's evaluation-policy question: compile-time const evaluation only, no side effects); reflection deferred — attributes are compiler/tooling metadata in v1.0. AC: `#[Test]`, `#[PhpExport]` representable.
- **Stage 33 — Baton MVP.** §11 scope: new/build/run/test/check, path deps, `#[Test]` runner. AC: `baton new game && baton test` green out of the box.

### Phase G — OOP completion (Stages 34–36)
- **Stage 34 — Inheritance.** D17: `open`/`override`, vtables, parent construction rules, devirtualization in LLVM profile. AC: `Post extends Model` native; missing-override diagnostics.
- **Stage 35 — Interfaces + traits.** Conformance checking, interface-typed values (fat pointer or vtable-embedded — decide in 0053/0054), trait flattening + `insteadof`/`as`. AC: SPEC §8 examples native.
- **Stage 36 — Property hooks + when.** §6.4 hooks; `when` grammar per 0064. AC: `Temperature` example; when-chain example.

### Phase H — Concurrency (Stages 37–39)
- **Stage 37 — Concurrency design record 0063.** Paper stage: full async model (executor in doria-rt, task groups, cancellation, `Shareable` rules). Designer sign-off required — this is the one deliberate design gate in the plan.
- **Stage 38 — async/await codegen.** State-machine lowering in MIR; single-threaded executor first. AC: async file-read example; interpreter parity.
- **Stage 39 — Structured task groups + Shareable checking.** Multi-threaded executor; spawn-boundary checks (readonly + Shareable). AC: parallel map example; data-race fixture rejected at compile time.

### Phase I — Systems and PHP bridge (Stages 40–42)
- **Stage 40 — unsafe/FFI.** D12: `unsafe`, `Ptr<T>`, `extern "C"`, linking foreign libs via Baton manifest. AC: bind and call a C function (e.g., zlib) from Doria.
- **Stage 41 — php-lib bridge.** D13c end-to-end: export analysis, C-ABI shim gen, PHP FFI stub gen, handle lifetime tests against real PHP 8 in CI. AC: the `ImageResizer` scenario runs from a PHP script in CI.
- **Stage 42 — migrate php v0.** §10.2 conservative converter. AC: converts a small idiomatic PHP 8 fixture app; dynamic features produce diagnostics not silent guesses.

### Phase J — Engine enablers and 1.0 hardening (Stages 43+)
- **Stage 43 — Engine profile.** declare-based overflow/exclusivity relaxation for audited modules; arena allocator hooks in doria-rt; benchmark suite (criterion-style) vs C/Rust baselines for ARC/collection hot paths.
- **Stage 44 — SIMD direction (0065)** + `std::net`/`http` maturation for the PHP sidecar pattern.
- **Stage 45 — Self-hosting start.** Port the lexer to Doria as the first self-hosted component (per docs/self-hosting.md), compiled by `doriac`, differentially tested against the Rust lexer.
- **1.0 gate:** spec freeze pass over SPEC.md, diagnostics audit, doria-rt ABI review, differential + fuzz suites green, the three flagship demos build: a small game (engine seed), a UI component demo, and a PHP app calling a Doria php-lib.

### Dependency notes for the implementing agent
- Nothing in Phases B–J may begin before Stage 11 lands (everything depends on MIR + oracle).
- WASM backend remains recognized-but-unscheduled; do not start it before 1.0.
- Game engine and UI framework are **separate repositories** consuming Doria; this plan only builds their enablers. Do not scaffold them inside the compiler repo.

---

## 14. What is explicitly out of scope for v1.0

Tracing GC (never), borrow/lifetime surface syntax (never), `Result<T,E>` surface model (per 0035), unions beyond `?T`, `protected`, `goto`, textual macros (per 0028), runtime reflection, package registry server, catchable panics, user-defined operator overloading, default interface methods, variadic generics, and bidirectional PHP compatibility guarantees.

---

## 15. Summary for the designer

This plan turns Doria's accepted principles into a complete, ordered build-out: an ARC + copy-on-write memory model that delivers Rust's safety outcomes in PHP-shaped spelling; a finished type system (fixed-width numerics, nullables, payload enums, exhaustive match, monomorphized generics); checked errors as the recoverable path and abort panics as the fatal one; closed-by-default OOP with traits and hooks; a real MIR with an interpreter oracle and dual Cranelift/LLVM backends over one semantics; a Doria-authored stdlib; Baton; and — as the strategic differentiator — a first-class native bridge that lets any PHP application call compiled Doria as if it were a normal PHP class. Approve or amend the Section 1 table, and the rest executes stage by stage without further design stalls.