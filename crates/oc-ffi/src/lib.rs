use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::Mutex;

use oc_backend::{Backend, ProtocolRegistry};
use oc_core::command::{UiCommand, UiResponse};
use oc_protocol_ftp::FtpProtocolFactory;
use oc_protocol_ftps::FtpsProtocolFactory;
use oc_protocol_sftp::SftpProtocolFactory;

type RawResponse = *mut c_char;

pub struct BackendHandle {
    runtime: tokio::runtime::Runtime,
    backend: Mutex<Backend>,
}

fn build_backend() -> Backend {
    let mut registry = ProtocolRegistry::default();
    registry.register(FtpProtocolFactory::new());
    registry.register(SftpProtocolFactory::new());
    registry.register(FtpsProtocolFactory::new());
    Backend::new(registry)
}

fn response_ptr(response: UiResponse) -> RawResponse {
    let serialized = match serde_json::to_string(&response) {
        Ok(json) => json,
        Err(error) => {
            format!(
                "{{\"status\":\"error\",\"code\":\"serialization_error\",\"message\":\"{}\"}}",
                error
            )
        }
    };

    let sanitized = serialized.replace('\0', " ");
    match CString::new(sanitized) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

fn error_ptr(message: &str) -> RawResponse {
    response_ptr(UiResponse::error("ffi_error", message))
}

#[no_mangle]
pub extern "C" fn oc_backend_new() -> *mut BackendHandle {
    match catch_unwind(AssertUnwindSafe(|| {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(_) => return ptr::null_mut(),
        };

        let handle = BackendHandle {
            runtime,
            backend: Mutex::new(build_backend()),
        };

        Box::into_raw(Box::new(handle))
    })) {
        Ok(ptr) => ptr,
        Err(_) => ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn oc_backend_execute(
    handle: *mut BackendHandle,
    command_json: *const c_char,
) -> RawResponse {
    if handle.is_null() || command_json.is_null() {
        return error_ptr("null pointer passed to oc_backend_execute");
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        let handle_ref = unsafe { &mut *handle };
        let command_cstr = unsafe { CStr::from_ptr(command_json) };

        let command_str = match command_cstr.to_str() {
            Ok(value) => value,
            Err(_) => return error_ptr("command_json must be valid UTF-8"),
        };

        let command: UiCommand = match serde_json::from_str(command_str) {
            Ok(command) => command,
            Err(error) => {
                return error_ptr(&format!("invalid command json: {error}"));
            }
        };

        let mut backend = match handle_ref.backend.lock() {
            Ok(backend) => backend,
            Err(_) => return error_ptr("backend lock poisoned"),
        };

        let response = handle_ref.runtime.block_on(backend.execute(command));
        response_ptr(response)
    }));

    match result {
        Ok(response) => response,
        Err(_) => error_ptr("panic while executing backend command"),
    }
}

#[no_mangle]
pub extern "C" fn oc_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        drop(CString::from_raw(ptr));
    }
}

#[no_mangle]
pub extern "C" fn oc_backend_free(handle: *mut BackendHandle) {
    if handle.is_null() {
        return;
    }

    unsafe {
        drop(Box::from_raw(handle));
    }
}
