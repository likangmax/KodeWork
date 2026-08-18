#![allow(unsafe_code)]
//! One-shot cleanup of stale test credentials (dev tool, not shipped).

use windows_sys::Win32::Foundation::TRUE;
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredEnumerateW, CredFree, CRED_TYPE_GENERIC,
};

fn main() {
    let mut count: u32 = 0;
    let mut credentials: *mut *mut windows_sys::Win32::Security::Credentials::CREDENTIALW =
        std::ptr::null_mut();
    let ok = unsafe { CredEnumerateW(std::ptr::null(), 0, &mut count, &mut credentials) };
    if ok != TRUE || credentials.is_null() {
        eprintln!("enumeration failed");
        std::process::exit(1);
    }
    let mut deleted = 0u32;
    for index in 0..count {
        let entry = unsafe { *credentials.add(index as usize) };
        if entry.is_null() {
            continue;
        }
        let raw = unsafe { (*entry).TargetName };
        let mut len = 0usize;
        while unsafe { *raw.add(len) } != 0 {
            len += 1;
        }
        let wide = unsafe { std::slice::from_raw_parts(raw, len) }.to_vec();
        let target = String::from_utf16_lossy(&wide);
        if target.starts_with("kodework-test/") {
            let delete_wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
            if unsafe { CredDeleteW(delete_wide.as_ptr(), CRED_TYPE_GENERIC, 0) } == TRUE {
                deleted += 1;
            } else {
                eprintln!("failed to delete {target:?}");
            }
        }
    }
    unsafe { CredFree(credentials as *mut core::ffi::c_void) };
    println!("deleted {deleted} stale credentials");
}
