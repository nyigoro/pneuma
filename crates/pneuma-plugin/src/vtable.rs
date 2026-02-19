#[repr(C)]
pub struct PneumaPluginVTable {
    pub abi_version: u32,
    pub plugin_name: extern "C" fn() -> *const std::ffi::c_char,
    pub initialize: extern "C" fn() -> bool,
    pub shutdown: extern "C" fn(),
}
