use core::ffi::{c_char, c_void};

// Mirrors the C `LV2_Descriptor` in example-lv2/src/plugin.c field-for-field.
#[repr(C)]
pub struct Lv2Descriptor {
    pub uri: *const c_char,
    pub instantiate: extern "C" fn(
        *const Lv2Descriptor,
        f64,
        *const c_char,
        *const *const c_void,
    ) -> *mut c_void,
    pub connect_port: extern "C" fn(*mut c_void, u32, *mut c_void),
    pub activate: extern "C" fn(*mut c_void),
    pub run: extern "C" fn(*mut c_void, u32),
    pub deactivate: extern "C" fn(*mut c_void),
    pub cleanup: extern "C" fn(*mut c_void),
    pub extension_data: extern "C" fn(*const c_char) -> *const c_void,
}
