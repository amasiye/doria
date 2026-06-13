use std::str::FromStr;

use crate::{codegen_php, ir};

pub trait Backend {
    fn emit(&self, program: &ir::Program) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendTarget {
    Native,
    Php,
    Debug,
    Wasm,
}

impl BackendTarget {
    pub fn name(self) -> &'static str {
        match self {
            BackendTarget::Native => "native",
            BackendTarget::Php => "php",
            BackendTarget::Debug => "debug",
            BackendTarget::Wasm => "wasm",
        }
    }

    pub fn is_available(self) -> bool {
        matches!(self, BackendTarget::Php)
    }

    pub fn description(self) -> &'static str {
        match self {
            BackendTarget::Native => "native machine code",
            BackendTarget::Php => "PHP compatibility/transpilation",
            BackendTarget::Debug => "debug interpreter",
            BackendTarget::Wasm => "WebAssembly",
        }
    }
}

impl FromStr for BackendTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "native" => Ok(BackendTarget::Native),
            "php" => Ok(BackendTarget::Php),
            "debug" => Ok(BackendTarget::Debug),
            "wasm" => Ok(BackendTarget::Wasm),
            _ => Err(format!("unknown backend target `{value}`")),
        }
    }
}

pub struct PhpBackend;

impl Backend for PhpBackend {
    fn emit(&self, program: &ir::Program) -> String {
        codegen_php::generate(program)
    }
}

pub fn emit(program: &ir::Program, target: BackendTarget) -> Result<String, String> {
    match target {
        BackendTarget::Php => Ok(PhpBackend.emit(program)),
        BackendTarget::Native | BackendTarget::Debug | BackendTarget::Wasm => Err(format!(
            "backend `{}` ({}) is planned but not implemented yet",
            target.name(),
            target.description()
        )),
    }
}
