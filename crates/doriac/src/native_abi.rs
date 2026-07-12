//! Shared helpers for the implementation-private native function ABI.

use crate::mir;

pub const STRING_FROM_UTF8: &str = "dr_v1_string_from_utf8";
pub const STRING_RETAIN: &str = "dr_v1_string_retain";
pub const STRING_RELEASE: &str = "dr_v1_string_release";
pub const STRING_CONCAT: &str = "dr_v1_string_concat";
pub const STRING_COMPARE: &str = "dr_v1_string_compare";
pub const STRING_DATA: &str = "dr_v1_string_data";
pub const STRING_LENGTH: &str = "dr_v1_string_length";
pub const STRING_WRITE_STDOUT: &str = "dr_v1_write_string_stdout";
pub const STRING_FROM_I64: &str = "dr_v1_string_from_i64";
pub const STRING_FROM_U64: &str = "dr_v1_string_from_u64";
pub const STRING_FROM_F32: &str = "dr_v1_string_from_f32";
pub const STRING_FROM_F64: &str = "dr_v1_string_from_f64";
pub const STRING_FROM_BOOL: &str = "dr_v1_string_from_bool";

pub fn function_symbol(function: &mir::Function) -> String {
    let sanitized = function
        .name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("__doria_fn_{}_{}", function.id.0, sanitized)
}
