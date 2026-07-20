use doriac::ast::{BinaryOp, Expr, Item, Stmt};

fn diagnostics(source: &str) -> Vec<doriac::diagnostics::Diagnostic> {
    doriac::check_source("stage22.doria", source).expect_err("source should be rejected")
}

fn assert_code(source: &str, code: &str) -> doriac::diagnostics::Diagnostic {
    let diagnostics = diagnostics(source);
    diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("expected {code}"))
}

fn snapshot_entry(source: &str, code: &str) -> String {
    let diagnostic = assert_code(source, code);
    format!(
        "[{}] {}\nhelp: {}\n",
        diagnostic.code,
        diagnostic.message,
        diagnostic.help.as_deref().unwrap_or("-")
    )
}

#[test]
fn parser_preserves_nullable_null_safe_coalesce_and_is_syntax() {
    let program = doriac::parse_source(
        "stage22-syntax.doria",
        r#"
class Label { function text(): string { return "label"; } }
function choose(?Label $label, mixed $value): string
{
    let $text = $label?->text() ?? "none";
    if ($value is string) { return $value; }
    return $text;
}
"#,
    )
    .expect("Stage 22 syntax should parse");

    let Item::Function(function) = &program.items[1] else {
        panic!("expected function");
    };
    assert!(function.params[0].ty.nullable);
    let Stmt::VarDecl(declaration) = &function.body.statements[0] else {
        panic!("expected declaration");
    };
    let Expr::Binary {
        left,
        op: BinaryOp::Coalesce,
        ..
    } = &declaration.initializer
    else {
        panic!("expected coalesce expression");
    };
    assert!(matches!(
        left.as_ref(),
        Expr::MethodCall {
            null_safe: true,
            ..
        }
    ));
    let Stmt::If(if_statement) = &function.body.statements[1] else {
        panic!("expected if statement");
    };
    assert!(matches!(if_statement.condition, Expr::IsType { .. }));
}

#[test]
fn nullable_members_require_narrowing_or_null_safe_access() {
    let diagnostic = assert_code(
        r#"
class Label { function text(): string { return "label"; } }
function read(?Label $label): string { return $label->text(); }
"#,
        "E0506",
    );
    assert!(diagnostic.message.contains("possibly-null"));

    doriac::check_source(
        "stage22-narrowed.doria",
        r#"
class Label { function text(): string { return "label"; } }
function read(?Label $label): string
{
    if ($label == null) { return "none"; }
    return $label->text();
}
function safe(?Label $label): string
{
    return $label?->text() ?? "none";
}
"#,
    )
    .expect("null guards and null-safe access should be accepted");
}

#[test]
fn narrowing_is_lexical_path_sensitive_and_short_circuit_aware() {
    doriac::check_source(
        "stage22-flow.doria",
        r#"
class Label { function text(): string { return "label"; } }
function read(?Label $label, mixed $value): string
{
    if ($label != null && $label->text() == "label") {
        let $value = 1;
        echo $value;
    }
    if ($value is string && $value == "text") {
        return $value;
    }
    return $label?->text() ?? "none";
}
"#,
    )
    .expect("narrowing should apply only to the selected binding and path");

    assert_code(
        r#"
class Label { function text(): string { return "label"; } }
function read(?Label $label): string
{
    if ($label != null) { echo $label->text(); }
    return $label->text();
}
"#,
        "E0506",
    );
}

#[test]
fn mixed_rejects_operations_until_is_narrows_the_value() {
    for (operation, expected_code) in [
        ("let $result = $value->name;", "E0433"),
        ("$value->show();", "E0433"),
        ("let $result = $value + 1;", "E0433"),
        ("let $result = $value . \"x\";", "E0433"),
        ("echo \"{$value}\";", "E0415"),
        ("let $result = $value == 1;", "E0433"),
    ] {
        let source = format!("function inspect(mixed $value): void {{ {operation} }}");
        let diagnostics = diagnostics(&source);
        let diagnostic = diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.code == expected_code)
            .unwrap_or_else(|| panic!("expected {expected_code} for `{operation}`"));
        if expected_code == "E0433" {
            assert!(diagnostic.help.is_some_and(|help| help.contains("`is`")));
        }
    }

    doriac::check_source(
        "stage22-mixed.doria",
        r#"
class Label
{
    string $name = "label";
    function show(): string { return $this->name; }
}
function describe(mixed $value): string
{
    if ($value is Label) { return $value->show() . $value->name; }
    if ($value is string) { return "{$value}" . $value; }
    if ($value is int && $value == 42) { return "number"; }
    return "other";
}
"#,
    )
    .expect("an is test should establish an exact type inside its branch");
}

#[test]
fn null_assignability_and_nullable_ownership_follow_the_payload_type() {
    doriac::check_source(
        "stage22-null-assignments.doria",
        r#"
class Label {}
function inspect(?int $left, ?int $right): void {}
function accepts(mixed $value): void {}
function valid(?Label $label): void
{
    ?int $number = null;
    ?string $text = null;
    inspect($number, $number);
    accepts(null);
}
"#,
    )
    .expect("null should enter nullable and mixed slots, and nullable scalars remain Copy");

    assert_code("int $value = null;", "E0403");

    let moved = diagnostics(
        r#"
class Label {}
function consume(take ?Label $label): void {}
function invalid(?Label $label): void
{
    consume($label);
    consume($label);
}
"#,
    );
    assert!(
        moved
            .iter()
            .any(|diagnostic| diagnostic.message.contains("given away")),
        "nullable classes should retain class move semantics: {moved:#?}"
    );
}

#[test]
fn mixed_runtime_representation_remains_a_stage23_boundary() {
    let diagnostics = doriac::lower_source_to_mir(
        "stage22-mixed-runtime.doria",
        r#"
function main(): void { mixed $value = 1; }
"#,
    )
    .expect_err("mixed runtime values should not lower before Stage 23");
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "M1101")
        .unwrap_or_else(|| panic!("expected native-stage diagnostic, got {diagnostics:#?}"));
    assert!(diagnostic.message.contains("Stage 23"));
}

#[test]
fn hierarchy_and_interface_is_tests_fail_at_their_owned_stages() {
    let hierarchy_source = r#"
class Base {}
class Child extends Base {}
function inspect(mixed $value): bool { return $value is Base; }
"#;
    let hierarchy_diagnostics = diagnostics(hierarchy_source);
    assert!(!hierarchy_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code.starts_with('P')));
    let hierarchy = hierarchy_diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.code == "E0509")
        .expect("expected hierarchy-stage diagnostic");
    assert!(hierarchy.message.contains("Stage 34"));

    let interface_source = "function inspect(mixed $value): bool { return $value is Displayable; }";
    let interface_diagnostics = diagnostics(interface_source);
    assert!(!interface_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code.starts_with('P')));
    let interface = interface_diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.code == "E0510")
        .expect("expected interface-stage diagnostic");
    assert!(interface.message.contains("Stage 35"));
}

#[test]
fn reserved_and_position_only_types_have_targeted_diagnostics() {
    assert_code("null $value = null;", "E0431");
    assert_code("void $value = null;", "E0430");
    assert_code("object $value = null;", "E0401");
    assert_code("resource $value = null;", "E0432");
}

#[test]
fn stage22_diagnostic_contract_matches_snapshot() {
    let snapshot = [
        snapshot_entry(
            r#"
class Label { function text(): string { return "label"; } }
function read(?Label $label): string { return $label->text(); }
"#,
            "E0506",
        ),
        snapshot_entry(
            "function add(mixed $value): int { return $value + 1; }",
            "E0433",
        ),
        snapshot_entry("null $value = null;", "E0431"),
        snapshot_entry("void $value = null;", "E0430"),
        snapshot_entry("object $value = null;", "E0401"),
        snapshot_entry("resource $value = null;", "E0432"),
    ]
    .concat();

    assert_eq!(
        snapshot,
        include_str!("fixtures/diagnostics/stage22_boundaries.txt")
    );
}

#[test]
fn nullable_native_fixture_lowers_to_valid_mir_and_interprets_exactly() {
    let source = include_str!("../../../examples/native/main_stage22_nullable.doria");
    let program = doriac::lower_source_to_mir("main_stage22_nullable.doria", source)
        .expect("Stage 22 fixture should lower");
    doriac::mir_validation::validate_program(&program).expect("Stage 22 MIR should validate");
    let output = doriac::mir_interpreter::interpret(&program)
        .expect("Stage 22 fixture should execute in the debug backend");
    assert_eq!(output.stdout, b"42:7:typed:text:empty:label:none:label\n");
    assert!(output.stderr.is_empty());
    assert_eq!(output.exit_status, 0);
}
