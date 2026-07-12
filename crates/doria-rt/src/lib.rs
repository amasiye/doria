#![cfg_attr(all(not(test), panic = "abort"), no_std)]

// Linked runtime artifacts always use panic=abort; unwind-mode builds exist only for check/test metadata.

use core::ffi::c_void;
use core::mem;
use core::ptr;

const PANIC_STATUS: i32 = 101;
#[cfg(unix)]
const EINTR: i32 = 4;
#[cfg(unix)]
const SIGPIPE: i32 = 13;
#[cfg(unix)]
const SIG_IGN: usize = 1;

#[repr(C)]
pub struct DrStackFrameV1 {
    pub parent: *const DrStackFrameV1,
    pub function_name: *const u8,
    pub function_name_length: usize,
}

/// Opaque outside doria-rt. Bytes immediately follow this header.
#[repr(C)]
pub struct DrStringV1 {
    references: usize,
    byte_length: usize,
}

const STRING_HEADER_SIZE: usize = mem::size_of::<DrStringV1>();

pub type DrMainIntV1 = unsafe extern "C" fn(*const DrStackFrameV1) -> i64;
pub type DrMainVoidV1 = unsafe extern "C" fn(*const DrStackFrameV1);

/// Invokes a generated Doria integer entry function and maps its result to a process status.
///
/// # Safety
///
/// `entry` must point to a generated function that implements `DrMainIntV1` and remains valid
/// for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_main_int(entry: DrMainIntV1) -> i32 {
    let status = entry(ptr::null());
    if (0..=125).contains(&status) {
        return status as i32;
    }

    static MAIN: &[u8] = b"main";
    static MESSAGE: &[u8] = b"main returned process status outside 0..125";
    let frame = DrStackFrameV1 {
        parent: ptr::null(),
        function_name: MAIN.as_ptr(),
        function_name_length: MAIN.len(),
    };
    dr_v1_panic(&frame, MESSAGE.as_ptr(), MESSAGE.len())
}

/// Invokes a generated Doria void entry function.
///
/// # Safety
///
/// `entry` must point to a generated function that implements `DrMainVoidV1` and remains valid
/// for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_main_void(entry: DrMainVoidV1) -> i32 {
    entry(ptr::null());
    0
}

/// Writes an exact byte sequence to stdout or panics when the write fails.
///
/// # Safety
///
/// `bytes` must be readable for `byte_length` bytes. `current_frame` must be null or point to a
/// valid `DrStackFrameV1` chain whose frame and function-name storage remains live for the call.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_write_stdout(
    current_frame: *const DrStackFrameV1,
    bytes: *const u8,
    byte_length: usize,
) {
    #[cfg(unix)]
    ignore_sigpipe();

    if write_stream(Stream::Stdout, bytes, byte_length) {
        return;
    }
    static MESSAGE: &[u8] = b"failed to write stdout";
    dr_v1_panic(current_frame, MESSAGE.as_ptr(), MESSAGE.len())
}

/// Writes an exact byte sequence to stderr.
///
/// # Safety
///
/// `bytes` must be readable for `byte_length` bytes.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_write_stderr(bytes: *const u8, byte_length: usize) {
    if !write_stream(Stream::Stderr, bytes, byte_length) {
        exit_process(PANIC_STATUS);
    }
}

/// Reports a fatal Doria panic and exits the process with status 101.
///
/// # Safety
///
/// `message` must be readable for `message_length` bytes. `current_frame` must be null or point to
/// a finite, valid `DrStackFrameV1` chain whose frames and function-name byte ranges remain live
/// until process termination.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_panic(
    current_frame: *const DrStackFrameV1,
    message: *const u8,
    message_length: usize,
) -> ! {
    write_panic_fragment(b"Panic: ");
    write_panic_bytes(message, message_length);
    write_panic_fragment(b"\nStack Trace:\n");

    let mut frame = current_frame;
    while !frame.is_null() {
        write_panic_fragment(b"  at ");
        write_panic_bytes((*frame).function_name, (*frame).function_name_length);
        write_panic_fragment(b"\n");
        frame = (*frame).parent;
    }
    exit_process(PANIC_STATUS)
}

/// Allocates an immutable runtime string from an explicit byte range.
///
/// # Safety
/// `bytes` must be readable for `byte_length` bytes and contain valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_string_from_utf8(
    bytes: *const u8,
    byte_length: usize,
) -> *mut DrStringV1 {
    let string = allocate_string(byte_length);
    if byte_length != 0 {
        ptr::copy_nonoverlapping(bytes, string_bytes_mut(string), byte_length);
    }
    string
}

unsafe fn allocate_string(byte_length: usize) -> *mut DrStringV1 {
    let total = STRING_HEADER_SIZE
        .checked_add(byte_length)
        .unwrap_or_else(|| string_runtime_panic(b"string length overflow"));
    let string = allocate(total).cast::<DrStringV1>();
    if string.is_null() {
        string_runtime_panic(b"string allocation failed");
    }
    ptr::write(
        string,
        DrStringV1 {
            references: 1,
            byte_length,
        },
    );
    string
}

/// Retains one owned reference.
///
/// # Safety
/// `string` must be null or a live doria-rt string.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_string_retain(string: *mut DrStringV1) -> *mut DrStringV1 {
    if !string.is_null() {
        (*string).references = (*string)
            .references
            .checked_add(1)
            .unwrap_or_else(|| string_runtime_panic(b"string reference count overflow"));
    }
    string
}

/// Releases one owned reference and frees the final reference.
///
/// # Safety
/// `string` must be null or a live owned doria-rt string reference.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_string_release(string: *mut DrStringV1) {
    if string.is_null() {
        return;
    }
    let references = (*string).references;
    if references == 0 {
        string_runtime_panic(b"string reference count underflow");
    }
    if references == 1 {
        deallocate(string.cast::<u8>());
    } else {
        (*string).references = references - 1;
    }
}

/// Concatenates two borrowed strings into a new owned string.
///
/// # Safety
/// Both pointers must identify live doria-rt strings.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_string_concat(
    left: *const DrStringV1,
    right: *const DrStringV1,
) -> *mut DrStringV1 {
    let length = (*left)
        .byte_length
        .checked_add((*right).byte_length)
        .unwrap_or_else(|| string_runtime_panic(b"string length overflow"));
    let result = allocate_string(length);
    ptr::copy_nonoverlapping(
        string_bytes(left),
        string_bytes_mut(result),
        (*left).byte_length,
    );
    ptr::copy_nonoverlapping(
        string_bytes(right),
        string_bytes_mut(result).add((*left).byte_length),
        (*right).byte_length,
    );
    result
}

/// Returns -1, 0, or 1 using unsigned byte-lexicographic ordering.
///
/// # Safety
/// Both pointers must identify live doria-rt strings.
#[no_mangle]
pub unsafe extern "C" fn dr_v1_string_compare(
    left: *const DrStringV1,
    right: *const DrStringV1,
) -> i32 {
    let common = core::cmp::min((*left).byte_length, (*right).byte_length);
    for index in 0..common {
        let left_byte = *string_bytes(left).add(index);
        let right_byte = *string_bytes(right).add(index);
        if left_byte < right_byte {
            return -1;
        }
        if left_byte > right_byte {
            return 1;
        }
    }
    match (*left).byte_length.cmp(&(*right).byte_length) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[no_mangle]
/// Returns the explicit byte pointer for a live string.
///
/// # Safety
/// `string` must identify a live doria-rt string for the duration of byte access.
pub unsafe extern "C" fn dr_v1_string_data(string: *const DrStringV1) -> *const u8 {
    string_bytes(string)
}

#[no_mangle]
/// Returns the explicit byte length for a live string.
///
/// # Safety
/// `string` must identify a live doria-rt string.
pub unsafe extern "C" fn dr_v1_string_length(string: *const DrStringV1) -> usize {
    (*string).byte_length
}

#[no_mangle]
/// Writes a borrowed string to stdout without adding a newline.
///
/// # Safety
/// `string` must identify a live doria-rt string and `current_frame` must be null or a valid frame chain.
pub unsafe extern "C" fn dr_v1_write_string_stdout(
    current_frame: *const DrStackFrameV1,
    string: *const DrStringV1,
) {
    dr_v1_write_stdout(current_frame, string_bytes(string), (*string).byte_length)
}

#[no_mangle]
/// Creates an owned string containing canonical signed decimal display text.
///
/// # Safety
/// The returned owned reference must eventually be released on a normal execution path.
pub unsafe extern "C" fn dr_v1_string_from_i64(value: i64) -> *mut DrStringV1 {
    let mut buffer = [0_u8; 20];
    let (start, length) = signed_decimal(value, &mut buffer);
    dr_v1_string_from_utf8(buffer.as_ptr().add(start), length)
}

#[no_mangle]
/// Creates an owned string containing canonical unsigned decimal display text.
///
/// # Safety
/// The returned owned reference must eventually be released on a normal execution path.
pub unsafe extern "C" fn dr_v1_string_from_u64(value: u64) -> *mut DrStringV1 {
    let mut buffer = [0_u8; 20];
    let (start, length) = unsigned_decimal(value, &mut buffer);
    dr_v1_string_from_utf8(buffer.as_ptr().add(start), length)
}

#[no_mangle]
/// Creates an owned string containing canonical binary32 display text.
///
/// # Safety
/// The returned owned reference must eventually be released on a normal execution path.
pub unsafe extern "C" fn dr_v1_string_from_f32(value: f32) -> *mut DrStringV1 {
    float_string_f32(value)
}

#[no_mangle]
/// Creates an owned string containing canonical binary64 display text.
///
/// # Safety
/// The returned owned reference must eventually be released on a normal execution path.
pub unsafe extern "C" fn dr_v1_string_from_f64(value: f64) -> *mut DrStringV1 {
    float_string_f64(value)
}

#[no_mangle]
/// Creates an owned string containing `true` or `false`.
///
/// # Safety
/// The returned owned reference must eventually be released on a normal execution path.
pub unsafe extern "C" fn dr_v1_string_from_bool(value: u8) -> *mut DrStringV1 {
    let bytes: &[u8] = if value == 0 { b"false" } else { b"true" };
    dr_v1_string_from_utf8(bytes.as_ptr(), bytes.len())
}

unsafe fn float_string_f32(value: f32) -> *mut DrStringV1 {
    if value.is_nan() {
        return string_from_static(b"NaN");
    }
    if value == f32::INFINITY {
        return string_from_static(b"Infinity");
    }
    if value == f32::NEG_INFINITY {
        return string_from_static(b"-Infinity");
    }
    if value == 0.0 {
        return string_from_static(if value.is_sign_negative() {
            b"-0"
        } else {
            b"0"
        });
    }
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format_finite(value);
    dr_v1_string_from_utf8(text.as_ptr(), text.len())
}

unsafe fn float_string_f64(value: f64) -> *mut DrStringV1 {
    if value.is_nan() {
        return string_from_static(b"NaN");
    }
    if value == f64::INFINITY {
        return string_from_static(b"Infinity");
    }
    if value == f64::NEG_INFINITY {
        return string_from_static(b"-Infinity");
    }
    if value == 0.0 {
        return string_from_static(if value.is_sign_negative() {
            b"-0"
        } else {
            b"0"
        });
    }
    let mut buffer = ryu::Buffer::new();
    let text = buffer.format_finite(value);
    dr_v1_string_from_utf8(text.as_ptr(), text.len())
}

unsafe fn string_from_static(bytes: &[u8]) -> *mut DrStringV1 {
    dr_v1_string_from_utf8(bytes.as_ptr(), bytes.len())
}

fn signed_decimal(value: i64, buffer: &mut [u8; 20]) -> (usize, usize) {
    let negative = value < 0;
    let magnitude = value.unsigned_abs();
    let (mut start, mut length) = unsigned_decimal(magnitude, buffer);
    if negative {
        start -= 1;
        buffer[start] = b'-';
        length += 1;
    }
    (start, length)
}

fn unsigned_decimal(mut value: u64, buffer: &mut [u8; 20]) -> (usize, usize) {
    let mut cursor = buffer.len();
    loop {
        cursor -= 1;
        buffer[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    (cursor, buffer.len() - cursor)
}

unsafe fn string_bytes(string: *const DrStringV1) -> *const u8 {
    string.cast::<u8>().add(STRING_HEADER_SIZE)
}

unsafe fn string_bytes_mut(string: *mut DrStringV1) -> *mut u8 {
    string.cast::<u8>().add(STRING_HEADER_SIZE)
}

unsafe fn string_runtime_panic(message: &[u8]) -> ! {
    dr_v1_panic(ptr::null(), message.as_ptr(), message.len())
}

#[cfg(unix)]
unsafe fn allocate(byte_length: usize) -> *mut u8 {
    malloc(byte_length).cast::<u8>()
}
#[cfg(unix)]
unsafe fn deallocate(memory: *mut u8) {
    free(memory.cast::<c_void>());
}

#[cfg(windows)]
unsafe fn allocate(byte_length: usize) -> *mut u8 {
    HeapAlloc(GetProcessHeap(), 0, byte_length).cast::<u8>()
}
#[cfg(windows)]
unsafe fn deallocate(memory: *mut u8) {
    let _ = HeapFree(GetProcessHeap(), 0, memory.cast::<c_void>());
}

#[cfg(not(any(unix, windows)))]
unsafe fn allocate(_byte_length: usize) -> *mut u8 {
    ptr::null_mut()
}
#[cfg(not(any(unix, windows)))]
unsafe fn deallocate(_memory: *mut u8) {}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

#[cfg(unix)]
unsafe fn ignore_sigpipe() {
    // Ignoring it makes write(2) report EPIPE instead of terminating the process by signal.
    signal(SIGPIPE, SIG_IGN);
}

unsafe fn write_panic_fragment(bytes: &[u8]) {
    write_panic_bytes(bytes.as_ptr(), bytes.len());
}

unsafe fn write_panic_bytes(bytes: *const u8, byte_length: usize) {
    if !write_stream(Stream::Stderr, bytes, byte_length) {
        exit_process(PANIC_STATUS);
    }
}

#[cfg(unix)]
unsafe fn write_stream(stream: Stream, bytes: *const u8, byte_length: usize) -> bool {
    let descriptor = match stream {
        Stream::Stdout => 1,
        Stream::Stderr => 2,
    };
    let mut offset = 0;
    while offset < byte_length {
        let written = write(
            descriptor,
            bytes.add(offset).cast::<c_void>(),
            byte_length - offset,
        );
        if written > 0 {
            offset += written as usize;
            continue;
        }
        if written < 0 && last_errno() == EINTR {
            continue;
        }
        return false;
    }
    true
}

#[cfg(windows)]
unsafe fn write_stream(stream: Stream, bytes: *const u8, byte_length: usize) -> bool {
    let standard_handle = match stream {
        Stream::Stdout => STD_OUTPUT_HANDLE,
        Stream::Stderr => STD_ERROR_HANDLE,
    };
    let handle = GetStdHandle(standard_handle);
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return false;
    }

    let mut offset = 0;
    while offset < byte_length {
        let request = core::cmp::min(byte_length - offset, u32::MAX as usize) as u32;
        let mut written = 0_u32;
        let succeeded = WriteFile(
            handle,
            bytes.add(offset).cast::<c_void>(),
            request,
            &mut written,
            ptr::null_mut(),
        );
        if succeeded == 0 || written == 0 {
            return false;
        }
        offset += written as usize;
    }
    true
}

#[cfg(not(any(unix, windows)))]
unsafe fn write_stream(_stream: Stream, _bytes: *const u8, _byte_length: usize) -> bool {
    false
}

#[cfg(unix)]
unsafe fn exit_process(status: i32) -> ! {
    _exit(status)
}

#[cfg(windows)]
unsafe fn exit_process(status: i32) -> ! {
    ExitProcess(status as u32)
}

#[cfg(not(any(unix, windows)))]
unsafe fn exit_process(_status: i32) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
unsafe fn last_errno() -> i32 {
    *__errno_location()
}

#[cfg(all(
    unix,
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )
))]
unsafe fn last_errno() -> i32 {
    *__error()
}

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))
))]
unsafe fn last_errno() -> i32 {
    0
}

#[cfg(unix)]
extern "C" {
    fn signal(signal: i32, handler: usize) -> usize;
    fn write(descriptor: i32, bytes: *const c_void, byte_length: usize) -> isize;
    fn _exit(status: i32) -> !;
    fn malloc(byte_length: usize) -> *mut c_void;
    fn free(memory: *mut c_void);
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
extern "C" {
    fn __errno_location() -> *mut i32;
}

#[cfg(all(
    unix,
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )
))]
extern "C" {
    fn __error() -> *mut i32;
}

#[cfg(windows)]
const STD_OUTPUT_HANDLE: u32 = -11_i32 as u32;
#[cfg(windows)]
const STD_ERROR_HANDLE: u32 = -12_i32 as u32;
#[cfg(windows)]
const INVALID_HANDLE_VALUE: *mut c_void = -1_isize as *mut c_void;

// Doria's Windows executables deliberately do not link the C runtime. Rust and ryu still lower
// byte copies/fills and floating-point use to these MSVC support symbols, so the runtime owns the
// small subset they require.
#[cfg(windows)]
#[no_mangle]
pub static _fltused: i32 = 0;

/// Copies `count` bytes from `source` to the non-overlapping `destination`.
///
/// # Safety
///
/// `source` and `destination` must be valid for `count` bytes and must not overlap.
#[cfg(windows)]
#[no_mangle]
pub unsafe extern "C" fn memcpy(
    destination: *mut c_void,
    source: *const c_void,
    count: usize,
) -> *mut c_void {
    let destination_bytes = destination.cast::<u8>();
    let source_bytes = source.cast::<u8>();
    for index in 0..count {
        let byte = ptr::read_volatile(source_bytes.add(index));
        ptr::write_volatile(destination_bytes.add(index), byte);
    }
    destination
}

/// Copies `count` bytes from `source` to `destination`, including when they overlap.
///
/// # Safety
///
/// `source` and `destination` must be valid for `count` bytes.
#[cfg(windows)]
#[no_mangle]
pub unsafe extern "C" fn memmove(
    destination: *mut c_void,
    source: *const c_void,
    count: usize,
) -> *mut c_void {
    let destination_bytes = destination.cast::<u8>();
    let source_bytes = source.cast::<u8>();
    let destination_address = destination_bytes as usize;
    let source_address = source_bytes as usize;

    if destination_address <= source_address
        || destination_address.wrapping_sub(source_address) >= count
    {
        for index in 0..count {
            let byte = ptr::read_volatile(source_bytes.add(index));
            ptr::write_volatile(destination_bytes.add(index), byte);
        }
    } else {
        for index in (0..count).rev() {
            let byte = ptr::read_volatile(source_bytes.add(index));
            ptr::write_volatile(destination_bytes.add(index), byte);
        }
    }
    destination
}

/// Fills `count` bytes at `destination` with the low byte of `value`.
///
/// # Safety
///
/// `destination` must be valid for writes of `count` bytes.
#[cfg(windows)]
#[no_mangle]
pub unsafe extern "C" fn memset(destination: *mut c_void, value: i32, count: usize) -> *mut c_void {
    let destination_bytes = destination.cast::<u8>();
    for index in 0..count {
        ptr::write_volatile(destination_bytes.add(index), value as u8);
    }
    destination
}

#[cfg(windows)]
extern "system" {
    fn GetStdHandle(standard_handle: u32) -> *mut c_void;
    fn WriteFile(
        handle: *mut c_void,
        bytes: *const c_void,
        byte_length: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn GetProcessHeap() -> *mut c_void;
    fn HeapAlloc(heap: *mut c_void, flags: u32, byte_length: usize) -> *mut c_void;
    fn HeapFree(heap: *mut c_void, flags: u32, memory: *mut c_void) -> i32;
    fn ExitProcess(status: u32) -> !;
}

#[cfg(all(not(test), panic = "abort"))]
#[panic_handler]
fn rust_panic(_information: &core::panic::PanicInfo<'_>) -> ! {
    unsafe { exit_process(PANIC_STATUS) }
}

#[cfg(all(not(test), panic = "abort"))]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn bytes(string: *const DrStringV1) -> &'static [u8] {
        core::slice::from_raw_parts(dr_v1_string_data(string), dr_v1_string_length(string))
    }

    #[test]
    fn stack_frame_layout_is_three_pointer_words() {
        assert_eq!(
            core::mem::size_of::<DrStackFrameV1>(),
            3 * core::mem::size_of::<usize>()
        );
        assert_eq!(
            core::mem::align_of::<DrStackFrameV1>(),
            core::mem::align_of::<usize>()
        );
    }

    #[test]
    fn explicit_lengths_preserve_empty_embedded_nul_and_utf8() {
        unsafe {
            for expected in [b"".as_slice(), b"a\0b".as_slice(), "Dória".as_bytes()] {
                let string = dr_v1_string_from_utf8(expected.as_ptr(), expected.len());
                assert_eq!(bytes(string), expected);
                dr_v1_string_release(string);
            }
        }
    }

    #[test]
    fn retain_release_and_concat_preserve_immutable_values() {
        unsafe {
            let left = dr_v1_string_from_utf8(b"Dor".as_ptr(), 3);
            let retained = dr_v1_string_retain(left);
            let right = dr_v1_string_from_utf8(b"ia".as_ptr(), 2);
            let joined = dr_v1_string_concat(left, right);
            assert_eq!(bytes(joined), b"Doria");
            assert_eq!(dr_v1_string_compare(left, retained), 0);
            dr_v1_string_release(left);
            dr_v1_string_release(retained);
            dr_v1_string_release(right);
            dr_v1_string_release(joined);
        }
    }

    #[test]
    fn canonical_primitive_display_is_exact() {
        unsafe {
            let cases = [
                (
                    dr_v1_string_from_i64(i64::MIN),
                    b"-9223372036854775808".as_slice(),
                ),
                (
                    dr_v1_string_from_u64(u64::MAX),
                    b"18446744073709551615".as_slice(),
                ),
                (dr_v1_string_from_bool(0), b"false".as_slice()),
                (dr_v1_string_from_bool(1), b"true".as_slice()),
                (dr_v1_string_from_f32(-0.0), b"-0".as_slice()),
                (dr_v1_string_from_f64(f64::NAN), b"NaN".as_slice()),
                (dr_v1_string_from_f64(f64::INFINITY), b"Infinity".as_slice()),
                (
                    dr_v1_string_from_f64(f64::NEG_INFINITY),
                    b"-Infinity".as_slice(),
                ),
            ];
            for (string, expected) in cases {
                assert_eq!(bytes(string), expected);
                dr_v1_string_release(string);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn no_crt_memory_support_symbols_preserve_bytes_and_overlap() {
        unsafe {
            let source = [1_u8, 2, 3, 4];
            let mut copied = [0_u8; 4];
            memcpy(
                copied.as_mut_ptr().cast(),
                source.as_ptr().cast(),
                source.len(),
            );
            assert_eq!(copied, source);

            memset(copied.as_mut_ptr().cast(), 0xab, copied.len());
            assert_eq!(copied, [0xab; 4]);

            let mut moved = [1_u8, 2, 3, 4, 5];
            memmove(moved.as_mut_ptr().add(1).cast(), moved.as_ptr().cast(), 4);
            assert_eq!(moved, [1, 1, 2, 3, 4]);

            memmove(moved.as_mut_ptr().cast(), moved.as_ptr().add(1).cast(), 4);
            assert_eq!(moved, [1, 2, 3, 4, 4]);
        }
    }
}
