# 0017 Stage 2c native integer arithmetic

Status: Accepted

Accepted by the Stage 2c implementation task. Keep this slice narrow; later native stages must still be accepted separately.

## Decision

Stage 2c adds native support for simple integer arithmetic in readonly integer local initializers inside the accepted native entrypoint:

```doria
function main(): int
{
    let $base = 20;
    let $code = $base * 2 + 2;
    return $code;
}
```

Supported Stage 2c arithmetic operators:

```text
+
-
*
```

Supported operands are:

```text
- integer literals
- prior supported readonly integer locals
- nested supported Stage 2c arithmetic expressions
```

The final return remains Stage 2b-shaped in Stage 2c:

```doria
return 42;
return $code;
```

Returned arithmetic expressions are accepted separately in `0018-stage-2d-native-returned-int-expressions.md`.

## Rationale

Stage 2c should prove integer arithmetic without broadening native execution into general expression lowering, control flow, assignments, function calls, or runtime arithmetic.

The implementation may track supported readonly integer expressions as compile-time values for this native slice. That is a narrow validation and lowering fact, not a general Doria `const` feature and not a Doria local storage model.

## Overflow and range

Doria `int` is signed 64-bit for early native integer semantics.

Compile-time overflow in supported integer arithmetic is a semantic diagnostic before Doria IR/native lowering.

The process exit-code range remains:

```text
0..125
```

That range applies only to the value returned from `main()` as the current portable native smoke-test process exit code. It is not the range of Doria `int` or local integer values.

Therefore, this is valid Stage 2c native output:

```doria
function main(): int
{
    let $negative = 1 - 2;
    return 0;
}
```

but this is rejected by the native backend until a broader process-exit mapping decision exists:

```doria
function main(): int
{
    let $code = 1 - 2;
    return $code;
}
```

## Division and modulo

Integer division and modulo are not part of Stage 2c.

They need an explicit Doria semantics decision before native output supports them, including behavior for:

```text
- division by zero
- modulo by zero
- negative dividends or divisors
- rounding/truncation direction
- overflow edge cases
```

The native backend must not silently inherit division or modulo behavior from Rust, Cranelift, LLVM, C, PHP, or the host platform.

## Accepted source forms

Stage 2c native output accepts:

```doria
function main(): int
{
    let $code = 20 + 22;
    return $code;
}
```

```doria
function main(): int
{
    int $base = 20;
    let $code = $base * 2 + 2;
    return $code;
}
```

```doria
function main(): int
{
    let $a = 50;
    let $b = $a - 8;
    return $b;
}
```

## Unsupported native forms

These forms may remain valid Doria but are not Stage 2c native output:

```doria
function main(): int
{
    return 20 + 22;
}
```

```doria
function main(): int
{
    let $code = 84 / 2;
    return $code;
}
```

```doria
function main(): int
{
    let writable $code = 0;
    $code = 20 + 22;
    return $code;
}
```

```doria
function main(): int
{
    let $code = 42;
    if ($code == 42) {
        return $code;
    }

    return 0;
}
```

## Non-goals

This decision does not:

- support direct returned arithmetic expressions; those are accepted separately in `0018-stage-2d-native-returned-int-expressions.md`
- support writable locals or assignments
- support compound assignments
- support division or modulo
- support runtime arithmetic over values not known in this narrow native slice
- support `if`, `while`, `foreach`, or other control flow in native output
- define final process-exit behavior for all Doria integer values
- define a general constant evaluation feature
- define a Doria local storage model
