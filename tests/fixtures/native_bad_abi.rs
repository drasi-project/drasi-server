// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use std::ffi::c_void;

#[repr(C)]
pub struct Header {
    magic: u64,
    major: u16,
    minor: u16,
    size: u32,
}

static HEADER: Header = Header {
    magic: 0x4452_4153_4943_4731,
    major: 99,
    minor: 0,
    size: 16,
};

fn mark(suffix: &str) {
    std::fs::write(
        format!("{}.{suffix}", env!("DRASI_BAD_ABI_MARKER")),
        b"called",
    )
    .unwrap();
}

extern "C" fn initialize() {
    mark("constructor");
}

#[used]
#[cfg_attr(target_os = "macos", link_section = "__DATA,__mod_init_func")]
#[cfg_attr(all(unix, not(target_os = "macos")), link_section = ".init_array")]
#[cfg_attr(windows, link_section = ".CRT$XCU")]
static INITIALIZE: extern "C" fn() = initialize;

#[no_mangle]
pub extern "C" fn drasi_computation_plugin_metadata() -> *const Header {
    mark("metadata");
    &HEADER
}

#[no_mangle]
pub extern "C" fn drasi_computation_plugin_entry() -> *const c_void {
    mark("native");
    std::ptr::null()
}

#[no_mangle]
pub extern "C" fn drasi_plugin_init() -> *const c_void {
    mark("legacy");
    std::ptr::null()
}
