fn assert_diagnostic(source: &str, code: &str) {
    let diagnostics =
        doriac::check_source("stage21.doria", source).expect_err("source should be rejected");
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("expected {code}, got {diagnostics:#?}"));
    assert!(!diagnostic.message.contains("lifetime"));
    assert!(!diagnostic.message.contains("borrow checker"));
}

#[test]
fn readonly_borrows_of_one_owner_can_overlap_in_a_call() {
    doriac::check_source(
        "stage21-readonly-overlap.doria",
        r#"
class Guard {}

function inspect(Guard $left, Guard $right): void {}

function route(take Guard $guard): void
{
    inspect($guard, $guard);
    inspect($guard, $guard);
}
"#,
    )
    .expect("many readonly uses of one owner may overlap");
}

#[test]
fn writable_and_readonly_uses_of_one_owner_conflict_in_a_call() {
    assert_diagnostic(
        r#"
class Guard {}

function touch(writable Guard $slot, Guard $view): void {}

function route(writable Guard $guard): void
{
    touch($guard, $guard);
}
"#,
        "E0477",
    );
}

#[test]
fn two_writable_uses_of_one_owner_conflict_in_a_call() {
    assert_diagnostic(
        r#"
class Guard {}

function swap(writable Guard $left, writable Guard $right): void {}

function route(writable Guard $guard): void
{
    swap($guard, $guard);
}
"#,
        "E0477",
    );
}

#[test]
fn writable_method_receiver_conflicts_with_reading_the_same_owner_as_an_argument() {
    assert_diagnostic(
        r#"
class Guard
{
    writable function copyFrom(Guard $other): void {}
}

function route(writable Guard $guard): void
{
    $guard->copyFrom($guard);
}
"#,
        "E0477",
    );
}

#[test]
fn outer_call_borrows_remain_live_while_later_arguments_are_evaluated() {
    assert_diagnostic(
        r#"
class Guard {}

function observe(Guard $guard, string $label): void {}
function label(writable Guard $guard): string { return "updated"; }

function route(writable Guard $guard): void
{
    observe($guard, label($guard));
}
"#,
        "E0477",
    );
}

#[test]
fn ordinary_call_borrows_end_after_the_statement() {
    doriac::check_source(
        "stage21-nll-call-end.doria",
        r#"
class Guard {}

function observe(Guard $guard): void {}
function update(writable Guard $guard): void {}

function route(writable Guard $guard): void
{
    observe($guard);
    update($guard);
    observe($guard);
}
"#,
    )
    .expect("non-lexical call borrows end after their last use");
}
