//! macOS-specific implementation for native plugins.

use std::ffi::c_void;
use std::path::Path;

use libc::{RTLD_NOW, c_int, dlclose, dlerror, dlopen, dlsym};

use crate::error::NativePluginError;

/// macOS-specific plugin handle.
pub struct MacosPluginHandle {
    handle: *mut c_void,
}

impl MacosPluginHandle {
    /// Load a plugin from a path.
    pub fn load(path: &Path) -> Result<Self, NativePluginError> {
        let path_cstr = std::ffi::CString::new(path.to_string_lossy().into_owned())
            .map_err(|_| NativePluginError::LoadingFailed("Invalid path".to_string()))?;

        let handle = unsafe { dlopen(path_cstr.as_ptr(), RTLD_NOW) };

        if handle.is_null() {
            let error = unsafe {
                let err_ptr = dlerror();
                if err_ptr.is_null() {
                    "Unknown dlopen error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(err_ptr)
                        .to_string_lossy()
                        .into_owned()
                }
            };
            return Err(NativePluginError::LoadingFailed(format!(
                "dlopen failed: {}",
                error
            )));
        }

        Ok(Self { handle })
    }

    /// Get a symbol from the plugin.
    pub fn get_symbol<T>(&self, name: &[u8]) -> Result<*mut T, NativePluginError> {
        let name_cstr = std::ffi::CString::new(name)
            .map_err(|_| NativePluginError::LoadingFailed("Invalid symbol name".to_string()))?;

        // Clear any previous error
        unsafe { dlerror() };

        let symbol = unsafe { dlsym(self.handle, name_cstr.as_ptr()) };

        if symbol.is_null() {
            let error = unsafe {
                let err_ptr = dlerror();
                if err_ptr.is_null() {
                    "Symbol not found".to_string()
                } else {
                    std::ffi::CStr::from_ptr(err_ptr)
                        .to_string_lossy()
                        .into_owned()
                }
            };
            return Err(NativePluginError::LoadingFailed(format!(
                "dlsym failed: {}",
                error
            )));
        }

        Ok(symbol as *mut T)
    }

    /// Check if running as root.
    pub fn is_root() -> bool {
        unsafe { libc::getuid() == 0 }
    }

    /// Check if running with elevated privileges.
    pub fn is_elevated() -> bool {
        Self::is_root()
    }
}

impl Drop for MacosPluginHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                dlclose(self.handle);
            }
        }
    }
}

/// Memory protection constants.
pub mod mem_protect {
    use libc::c_int;

    /// Readable.
    pub const READ: c_int = libc::PROT_READ;
    /// Writable.
    pub const WRITE: c_int = libc::PROT_WRITE;
    /// Executable.
    pub const EXEC: c_int = libc::PROT_EXEC;
}

/// Change memory protection for a region.
///
/// # Safety
///
/// The `address` must be a valid pointer to memory that can be protected.
/// The `size` must be the correct size of the memory region.
pub unsafe fn protect_memory(
    address: *mut c_void,
    size: usize,
    prot: c_int,
) -> Result<(), NativePluginError> {
    let page_size = libc::sysconf(libc::_SC_PAGESIZE) as usize;
    let page_mask = !(page_size - 1);
    let aligned_addr = (address as usize) & page_mask;
    let aligned_size = ((address as usize) + size - aligned_addr + page_size - 1) & page_mask;

    let result = libc::mprotect(aligned_addr as *mut c_void, aligned_size, prot);

    if result != 0 {
        return Err(NativePluginError::LoadingFailed(format!(
            "mprotect failed: {}",
            std::io::Error::last_os_error()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_root() {
        let _ = MacosPluginHandle::is_root();
    }
}
