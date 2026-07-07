pub mod ast;
pub mod backend;
pub mod codegen_native;
pub mod codegen_php;
pub mod diagnostics;
pub mod hir;
pub mod lexer;
pub mod lowering;
pub mod lsp;
pub mod mir;
pub mod mir_interpreter;
pub mod mir_lowering;
mod native_smoke;
pub mod parser;
pub mod semantics;
pub mod source;
pub mod symbols;
pub mod types;

use ast::Program;
use backend::BackendTarget;
use diagnostics::{Diagnostic, DiagnosticResult};
use source::{SourceFile, Span};

pub fn lex_source(
    path: impl Into<String>,
    text: impl Into<String>,
) -> DiagnosticResult<Vec<lexer::Token>> {
    let source = SourceFile::new(path, text);
    lexer::Lexer::new(&source).lex()
}

pub fn parse_source(path: impl Into<String>, text: impl Into<String>) -> DiagnosticResult<Program> {
    let source = SourceFile::new(path, text);
    parse_source_file(&source)
}

pub fn check_source(path: impl Into<String>, text: impl Into<String>) -> DiagnosticResult<Program> {
    let source = SourceFile::new(path, text);
    let program = parse_source_file(&source)?;
    semantics::check_program(&program)?;
    Ok(program)
}

pub fn compile_source_to_php(
    path: impl Into<String>,
    text: impl Into<String>,
) -> DiagnosticResult<String> {
    match compile_source(path, text, BackendTarget::Php)? {
        backend::BackendOutput::Text { contents, .. } => Ok(contents),
        _ => Err(vec![Diagnostic::new(
            "B0002",
            "PHP backend did not return text output",
            Span::default(),
        )]),
    }
}

pub fn lower_source(
    path: impl Into<String>,
    text: impl Into<String>,
) -> DiagnosticResult<hir::Program> {
    let program = check_source(path, text)?;
    Ok(lowering::lower_program(&program))
}

pub fn lower_source_to_mir(
    path: impl Into<String>,
    text: impl Into<String>,
) -> DiagnosticResult<mir::Program> {
    let hir = lower_source(path, text)?;
    mir_lowering::lower_program(&hir)
}

pub fn compile_source(
    path: impl Into<String>,
    text: impl Into<String>,
    target: BackendTarget,
) -> Result<backend::BackendOutput, Vec<Diagnostic>> {
    let hir = lower_source(path, text)?;
    if target == BackendTarget::Debug {
        return compile_hir_to_debug_output(&hir);
    }

    backend::emit(&hir, target)
        .map_err(|error| vec![Diagnostic::new("B0001", error.message, Span::default())])
}

pub fn parse_source_file(source: &SourceFile) -> DiagnosticResult<Program> {
    let tokens = lexer::Lexer::new(source).lex()?;
    parser::Parser::new(tokens).parse_program()
}

pub fn render_diagnostics(
    path: impl Into<String>,
    text: impl Into<String>,
    diagnostics: &[Diagnostic],
) -> String {
    let source = SourceFile::new(path, text);
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.render(&source))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compile_hir_to_debug_output(
    hir: &hir::Program,
) -> Result<backend::BackendOutput, Vec<Diagnostic>> {
    let mir = mir_lowering::lower_program(hir)?;
    let output = mir_interpreter::interpret(&mir).map_err(|error| {
        vec![Diagnostic::new(
            "M1102",
            format!("MIR interpreter failure: {}", error.message),
            Span::default(),
        )]
    })?;

    Ok(backend::BackendOutput::Text {
        extension: "debug".to_string(),
        contents: mir_interpreter::render_debug_output(&output),
    })
}
