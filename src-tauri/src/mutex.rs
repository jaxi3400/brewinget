use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicPtr, Ordering};

const MUTEX_NAME: &str = "Local\\BrewingetUI";
const MUTEX_ALL_ACCESS: u32 = 0x1F_0001;

// kernel32 is automatically linked on all Windows Rust targets.
extern "system" {
    fn CreateMutexW(
        lp_mutex_attributes: *mut std::ffi::c_void,
        b_initial_owner: i32,
        lp_name: *const u16,
    ) -> *mut std::ffi::c_void;

    fn OpenMutexW(
        dw_desired_access: u32,
        b_inherit_handle: i32,
        lp_name: *const u16,
    ) -> *mut std::ffi::c_void;

    fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
}

// The mutex handle is intentionally held for the lifetime of the process.
// The OS releases it automatically when the process exits, even on crash.
static UI_MUTEX_HANDLE: AtomicPtr<std::ffi::c_void> =
    AtomicPtr::new(std::ptr::null_mut());

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Create the named UI mutex and hold it for the lifetime of the process.
/// Call once at Tauri startup. Safe to call again — CreateMutexW is idempotent.
pub fn acquire_ui_mutex() {
    let name = wide(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr()) };
    // Store regardless of null — if CreateMutexW failed we just have null,
    // which ui_is_running() will then also fail to open (safe false-negative).
    UI_MUTEX_HANDLE.store(handle, Ordering::SeqCst);
}

/// Returns true if the Brewinget UI is currently running.
/// Headless mode uses this to skip the update run gracefully.
pub fn ui_is_running() -> bool {
    let name = wide(MUTEX_NAME);
    let handle = unsafe { OpenMutexW(MUTEX_ALL_ACCESS, 0, name.as_ptr()) };
    if handle.is_null() {
        false
    } else {
        unsafe { CloseHandle(handle) };
        true
    }
}
