use doriac::ast::{ClassMember, Item, MemberAccess};
use doriac::const_eval::{ConstKey, ConstValue};
use doriac::mir;

const PARSER_EXAMPLE: &str = include_str!("../../../examples/native/main_stage20_parser.doria");
const DISPLAYABLE_EXAMPLE: &str =
    include_str!("../../../examples/native/main_stage20_displayable.doria");
const STATICS_EXAMPLE: &str = include_str!("../../../examples/native/main_stage20_statics.doria");

fn diagnostics(source: &str) -> Vec<doriac::diagnostics::Diagnostic> {
    doriac::check_source("stage20.doria", source).expect_err("source should be rejected")
}

fn assert_diagnostic(source: &str, code: &str) {
    let found = diagnostics(source);
    assert!(
        found.iter().any(|diagnostic| diagnostic.code == code),
        "expected {code}, got {found:#?}"
    );
}

fn lower(source: &str) -> mir::Program {
    doriac::lower_source_to_mir("stage20.doria", source).expect("source should lower to MIR")
}

fn interpret(source: &str) -> doriac::mir_interpreter::InterpreterOutput {
    doriac::mir_interpreter::interpret(&lower(source)).expect("MIR should interpret")
}

#[test]
fn parses_constants_static_members_and_explicit_method_identity() {
    let program = doriac::parse_source(
        "surface.doria",
        r#"
const int TOP_LIMIT = 40;

class Counter
{
    internal const STEP = TOP_LIMIT / 20;
    static int $initial = TOP_LIMIT;
    internal static writable int $value = Counter::initial;

    static function read(): int { return Counter::value; }
    writable function increment(): void { return; }
}
"#,
    )
    .expect("Stage 20 declarations should parse");

    assert!(matches!(program.items.first(), Some(Item::Constant(_))));
    let class = program
        .items
        .iter()
        .find_map(|item| match item {
            Item::Class(class) => Some(class),
            _ => None,
        })
        .expect("Counter class");
    assert!(class.members.iter().any(|member| matches!(
        member,
        ClassMember::Constant(constant)
            if constant.name == "STEP" && constant.access == MemberAccess::Internal
    )));
    assert!(class.members.iter().any(|member| matches!(
        member,
        ClassMember::Property(property)
            if property.name == "value" && property.is_static && property.writable
    )));
    assert!(class.members.iter().any(|member| matches!(
        member,
        ClassMember::Method(method)
            if method.name == "read" && method.is_static && !method.writable_this
    )));
    assert!(class.members.iter().any(|member| matches!(
        member,
        ClassMember::Method(method)
            if method.name == "increment" && !method.is_static && method.writable_this
    )));
}

#[test]
fn evaluates_typed_inferred_and_forward_constants_without_runtime_storage() {
    let hir = doriac::lower_source(
        "constants.doria",
        r#"
const int ANSWER = LATER + 1;
const LATER = 41;
const string LABEL = "Dor" . "ia";

class Limits
{
    const DOUBLE = ANSWER * 2;
}

function main(): void
{
    echo LABEL;
    echo Limits::DOUBLE;
}
"#,
    )
    .expect("forward constants should lower");
    let values = &hir.semantic_info.const_evaluation.values;
    let answer = &values[&ConstKey::TopLevel("ANSWER".to_string())].value;
    let later = &values[&ConstKey::TopLevel("LATER".to_string())].value;
    let doubled = &values[&ConstKey::Class {
        class_name: "Limits".to_string(),
        name: "DOUBLE".to_string(),
    }]
        .value;
    assert!(matches!(answer, ConstValue::Integer(value) if value.signed_value() == 42));
    assert!(matches!(later, ConstValue::Integer(value) if value.signed_value() == 41));
    assert!(matches!(doubled, ConstValue::Integer(value) if value.signed_value() == 84));

    let mir = doriac::mir_lowering::lower_program(&hir).expect("constants should fold into MIR");
    assert!(
        mir.statics.is_empty(),
        "constants must not allocate static storage"
    );
    let output = doriac::mir_interpreter::interpret(&mir).expect("folded constants should run");
    assert_eq!(output.stdout, b"Doria84");
}

#[test]
fn rejects_invalid_constant_dependencies_operations_and_names() {
    let cases = [
        ("const FIRST = SECOND; const SECOND = FIRST;", "E0482"),
        ("const int8 TOO_LARGE = 127 + 1;", "E0485"),
        (
            "function runtime(): int { return 1; } const VALUE = runtime();",
            "E0485",
        ),
        ("const int VALUE = \"wrong\";", "E0484"),
        ("const not_upper = 1;", "E0490"),
        ("const VALUE = 1; const VALUE = 2;", "E0481"),
    ];
    for (source, code) in cases {
        assert_diagnostic(source, code);
    }

    let cycle = diagnostics("const FIRST = SECOND; const SECOND = FIRST;");
    assert!(cycle.iter().any(
        |diagnostic| diagnostic.message.contains("FIRST -> SECOND -> FIRST")
            || diagnostic.message.contains("SECOND -> FIRST -> SECOND")
    ));
}

#[test]
fn enforces_static_initialization_and_copy_type_rules() {
    doriac::check_source(
        "valid-statics.doria",
        r#"
class Counter
{
    static int $initial = 40;
    static writable int $value = Counter::initial + 2;
    static string $label = "ready";
}

function main(): void
{
    Counter::value = 43;
    echo Counter::initial;
    echo Counter::value;
    echo Counter::label;
}
"#,
    )
    .expect("Copy statics with const-evaluable initializers should be accepted");

    let independent = interpret(
        r#"
class Left { static writable int $value = 1; }
class Right { static writable int $value = 2; }
function main(): void
{
    Left::value = 3;
    echo Left::value;
    echo Right::value;
}
"#,
    );
    assert_eq!(independent.stdout, b"32");

    assert_diagnostic(
        "class Counter { static int $value = 1; } function main(): void { Counter::value = 2; }",
        "E0202",
    );
    assert_diagnostic(
        "class Item {} class Store { static Item $item = new Item(); }",
        "E0486",
    );

    let runtime = diagnostics(
        "function value(): int { return 1; } class Store { static int $value = value(); }",
    );
    assert!(runtime.iter().any(|diagnostic| {
        diagnostic.code == "E0485"
            && diagnostic
                .message
                .contains("runtime-initialized statics require a future accepted decision record")
    }));
    assert_diagnostic(
        "class Store { static int $left = Store::right; static int $right = Store::left; }",
        "E0482",
    );
    let mutable_dependency = diagnostics(
        "class Store { static writable int $source = 1; static int $copy = Store::source; }",
    );
    assert!(mutable_dependency.iter().any(|diagnostic| {
        diagnostic.code == "E0485"
            && diagnostic
                .message
                .contains("constant evaluation cannot read writable static `Store::source`")
    }));
}

#[test]
fn internal_access_is_limited_to_the_declaring_class_for_every_member_kind() {
    doriac::check_source(
        "same-class.doria",
        r#"
class Vault
{
    internal const CODE = 2;
    internal int $secret = 40;
    internal static int $offset = Vault::CODE;

    internal function reveal(): int { return $this->secret; }
    internal static function staticOffset(): int { return Vault::offset; }

    function total(): int
    {
        return $this->reveal() + Vault::staticOffset();
    }
}
"#,
    )
    .expect("a class may access its own internal members");

    for (source, code) in [
        (
            "class Vault { internal int $secret = 1; } function main(): void { let $vault = new Vault(); echo $vault->secret; }",
            "E0306",
        ),
        (
            "class Vault { internal function reveal(): int { return 1; } } function expose(Vault $vault): int { return $vault->reveal(); }",
            "E0307",
        ),
        (
            "class Vault { internal static function reveal(): int { return 1; } } function main(): void { echo Vault::reveal(); }",
            "E0307",
        ),
        (
            "class Vault { internal const CODE = 1; } class Other { function expose(): int { return Vault::CODE; } }",
            "E0307",
        ),
        (
            "class Vault { internal static int $value = 1; } function main(): void { echo Vault::value; }",
            "E0307",
        ),
        (
            "class Vault { internal function __construct() {} } function main(): void { let $vault = new Vault(); }",
            "E0307",
        ),
    ] {
        assert_diagnostic(source, code);
    }
}

#[test]
fn lifecycle_methods_remain_non_static_and_non_callable() {
    for (source, code) in [
        ("class Item { static function __construct() {} }", "E0465"),
        ("class Item { static function __destruct() {} }", "E0465"),
        (
            "class Item { function __construct() {} } function main(): void { Item::__construct(); }",
            "E0414",
        ),
        (
            "class Item { function __destruct() {} } function main(): void { let $item = new Item(); $item->__destruct(); }",
            "E0414",
        ),
    ] {
        let found = diagnostics(source);
        assert!(
            found.iter().any(|diagnostic| diagnostic.code == code),
            "expected lifecycle diagnostic, got {found:#?}"
        );
    }
}

#[test]
fn method_calls_support_recursion_class_returns_moves_and_deterministic_drops() {
    let source = r#"
class Token
{
    function __construct(string $name) {}
    function __destruct() { echo "drop " . $this->name . "\n"; }
}

class Worker
{
    function sum(int $value): int
    {
        if ($value <= 0) { return 0; }
        return $value + $this->sum($value - 1);
    }

    function make(string $name): Token { return new Token($name); }
    function relay(take Token $token): Token { return $token; }
    function inspect(Token $token): string { return $token->name; }

    function leaveEarly(): void
    {
        let $local = new Token("local");
        return;
    }
}

function main(): void
{
    let $worker = new Worker();
    let $token = $worker->relay($worker->make("owned"));
    echo $worker->sum(6);
    echo ":" . $worker->inspect($token) . "\n";
    $worker->leaveEarly();
}
"#;
    let output = interpret(source);
    assert_eq!(output.stdout, b"21:owned\ndrop local\ndrop owned\n");
    assert!(output.stderr.is_empty());
    assert_eq!(output.exit_status, 0);
    assert!(
        !doriac::codegen_cranelift::lower_mir_to_object(&lower(source))
            .expect("method ownership source should lower to Cranelift")
            .is_empty()
    );
    #[cfg(feature = "llvm-backend")]
    assert!(!doriac::codegen_llvm::lower_mir_to_object(&lower(source))
        .expect("method ownership source should lower to LLVM")
        .is_empty());
}

#[test]
fn owned_property_replacement_remains_behind_the_writable_path_move_boundary() {
    assert_diagnostic(
        r#"
class Token {}

class Box
{
    writable Token $token = new Token();

    writable function replace(take Token $replacement): void
    {
        $this->token = $replacement;
    }
}
"#,
        "E0472",
    );
}

#[test]
fn property_initializers_may_call_internal_static_methods_of_the_declaring_class() {
    let source = r#"
class Message
{
    string $text = Message::defaultText();

    internal static function defaultText(): string
    {
        return "ready";
    }
}

function main(): void
{
    let $message = new Message();
    echo $message->text;
}
"#;
    let output = interpret(source);
    assert_eq!(output.stdout, b"ready");
    assert!(output.stderr.is_empty());
}

#[test]
fn panic_inside_a_method_keeps_the_accepted_no_cleanup_behavior() {
    let source = r#"
class Token
{
    function __destruct() { echo "unexpected cleanup"; }
}

class Runner
{
    function fail(): void
    {
        let $token = new Token();
        panic("method failed");
    }
}

function main(): void
{
    let $runner = new Runner();
    $runner->fail();
}
"#;
    let output = interpret(source);
    assert!(output.stdout.is_empty());
    assert_eq!(output.exit_status, 101);
    let stderr = String::from_utf8(output.stderr).expect("panic output is UTF-8");
    assert!(stderr.contains("Panic: method failed"));
    assert!(stderr.contains("Runner::fail"));
}

#[test]
fn writable_method_calls_require_writable_receivers() {
    let source = r#"
class Counter
{
    writable int $value = 0;
    writable function increment(): void { $this->value++; }
}

function main(): void
{
    let $counter = new Counter();
    $counter->increment();
}
"#;
    assert_diagnostic(source, "E0203");
}

#[test]
fn mir_records_receiver_modes_and_rejects_malformed_method_calls() {
    let program = lower(PARSER_EXAMPLE);
    let create = program
        .functions
        .iter()
        .find(|function| function.name.ends_with("::create"))
        .expect("static create method");
    let parse = program
        .functions
        .iter()
        .find(|function| function.name.ends_with("::parse"))
        .expect("writable parse method");
    let parse_program = program
        .functions
        .iter()
        .find(|function| function.name.ends_with("::parseProgram"))
        .expect("readonly parseProgram method");
    assert_eq!(create.receiver_mode, None);
    assert_eq!(parse.receiver_mode, Some(mir::ReceiverMode::Writable));
    assert_eq!(
        parse_program.receiver_mode,
        Some(mir::ReceiverMode::Readonly)
    );

    let mut missing_receiver = program.clone();
    let parse = missing_receiver
        .functions
        .iter_mut()
        .find(|function| function.name.ends_with("::parse"))
        .expect("writable parse method");
    parse.params.remove(0);
    let error = doriac::mir_validation::validate_program(&missing_receiver)
        .expect_err("method without a receiver parameter must be rejected");
    assert!(error.message.contains("has no receiver parameter"));

    let mut readonly_receiver = program;
    let main = readonly_receiver
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .expect("main function");
    let parser = main
        .locals
        .iter_mut()
        .find(|local| local.name == "parser")
        .expect("parser local");
    parser.writable = false;
    let error = doriac::mir_validation::validate_program(&readonly_receiver)
        .expect_err("writable method call through readonly MIR must be rejected");
    assert!(error.message.contains("requires a writable class value"));
}

#[test]
fn malformed_static_writes_are_rejected_by_shared_mir_validation() {
    let mut program = lower(STATICS_EXAMPLE);
    let value = program
        .statics
        .iter_mut()
        .find(|property| property.name == "value")
        .expect("writable value static");
    value.writable = false;
    let error = doriac::mir_validation::validate_program(&program)
        .expect_err("MIR cannot assign to a readonly static");
    assert!(error.message.contains("assignment targets readonly static"));
}

#[test]
fn stage20_acceptance_examples_run_through_the_shared_interpreter() {
    for (source, expected) in [
        (PARSER_EXAMPLE, b"Doria:parser\n".as_slice()),
        (DISPLAYABLE_EXAMPLE, b"L!R![LR]L!R!LRL!LL!R!LR\n".as_slice()),
        (STATICS_EXAMPLE, b"40:42:42:44:S:ready\n".as_slice()),
    ] {
        let output = interpret(source);
        assert_eq!(output.stdout, expected);
        assert!(output.stderr.is_empty());
        assert_eq!(output.exit_status, 0);
    }
}
