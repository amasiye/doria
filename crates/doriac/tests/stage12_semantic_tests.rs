use doriac::ast::Item;
use doriac::control_flow::NodeKind;

fn assert_valid(source: &str) {
    doriac::check_source("test.doria", source).expect("source should pass semantic checking");
}

fn assert_missing_return(source: &str) {
    let diagnostics =
        doriac::check_source("test.doria", source).expect_err("source should miss a return path");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E0406"),
        "expected E0406, got {diagnostics:?}"
    );
}

#[test]
fn both_if_branches_may_return_without_a_final_return() {
    assert_valid(
        r#"function answer(int $value): int
{
    if ($value == 42) {
        return 42;
    } else {
        return 0;
    }
}
"#,
    );
}

#[test]
fn nested_branches_may_return_on_every_path() {
    assert_valid(
        r#"function answer(int $value): int
{
    if ($value < 0) {
        return 0;
    } else if ($value == 42) {
        return 42;
    } else {
        return 1;
    }
}
"#,
    );
}

#[test]
fn guard_if_may_precede_a_fallback_return() {
    assert_valid(
        r#"function answer(int $value): int
{
    if ($value == 42) {
        return 42;
    }

    return 0;
}
"#,
    );
}

#[test]
fn missing_else_or_fallback_is_rejected() {
    assert_missing_return(
        r#"function answer(int $value): int
{
    if ($value == 42) {
        return 42;
    }
}
"#,
    );
}

#[test]
fn panic_is_a_diverging_int_function_path() {
    assert_valid(
        r#"function fail(): int
{
    panic("no value");
}
"#,
    );
}

#[test]
fn constant_true_loop_without_break_is_diverging() {
    assert_valid(
        r#"function neverReturns(): int
{
    while (true) {
    }
}
"#,
    );
}

#[test]
fn reachable_break_from_constant_true_loop_requires_a_return() {
    assert_missing_return(
        r#"function answer(int $value): int
{
    while (true) {
        if ($value == 42) {
            break;
        }
    }
}
"#,
    );
}

#[test]
fn return_after_loop_satisfies_the_break_path() {
    assert_valid(
        r#"function answer(int $value): int
{
    while (true) {
        if ($value == 42) {
            break;
        }
    }

    return 42;
}
"#,
    );
}

#[test]
fn nested_loop_return_exits_the_function() {
    assert_valid(
        r#"function answer(): int
{
    while (true) {
        while (true) {
            return 42;
        }
    }
}
"#,
    );
}

#[test]
fn void_fallthrough_and_main_implicit_success_remain_valid() {
    assert_valid(
        r#"function helper(): void
{
}

function main(): void
{
    helper();
}
"#,
    );
}

#[test]
fn panic_is_not_user_declarable() {
    let diagnostics = doriac::check_source(
        "test.doria",
        r#"function panic(string $message): void
{
}
"#,
    )
    .expect_err("panic redeclaration should fail");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "E0310" && diagnostic.message.contains("cannot be redeclared")
    }));
}

#[test]
fn panic_requires_one_compile_time_known_string_argument() {
    let wrong_arity = doriac::check_source(
        "test.doria",
        r#"function main(): void
{
    panic();
}
"#,
    )
    .expect_err("panic without a message should fail");
    assert!(wrong_arity
        .iter()
        .any(|diagnostic| diagnostic.code == "E0434"));

    let wrong_type = doriac::check_source(
        "test.doria",
        r#"function main(): void
{
    panic(42);
}
"#,
    )
    .expect_err("panic with a non-string message should fail");
    assert!(wrong_type
        .iter()
        .any(|diagnostic| diagnostic.code == "E0435"));

    let runtime_string = doriac::check_source(
        "test.doria",
        r#"function main(): void
{
    let writable $message = "boom";
    panic($message);
}
"#,
    )
    .expect_err("panic with a writable runtime message should fail");
    assert!(runtime_string
        .iter()
        .any(|diagnostic| diagnostic.code == "E0435"));
}

#[test]
fn cfg_nodes_retain_source_spans() {
    let program = doriac::parse_source(
        "test.doria",
        r#"function answer(int $value): int
{
    if ($value == 42) {
        return 42;
    }
    return 0;
}
"#,
    )
    .expect("source should parse");
    let function = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(function) => Some(function),
            _ => None,
        })
        .expect("function should exist");
    let analysis = doriac::return_analysis::analyze(function);
    let branch = analysis
        .graph
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Branch)
        .expect("CFG should contain a branch");
    assert!(branch.span.end > branch.span.start);
    assert!(!analysis.fallthrough_reachable);
}
