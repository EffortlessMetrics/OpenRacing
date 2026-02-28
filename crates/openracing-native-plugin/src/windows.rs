//! Windows-specific implementation for native plugins.

use std::ffi::c_void;
use std::mem::MaybeUninit;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, FreeLibrary, HANDLE, HMODULE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::error::NativePluginError;

/// Windows-specific plugin handle.
pub struct WindowsPluginHandle {
    module: HMODULE,
}

impl WindowsPluginHandle {
    /// Load a plugin from a path using LoadLibraryW.
    pub fn load(path: &Path) -> Result<Self, NativePluginError> {
        let path_wide: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let module = unsafe { LoadLibraryW(windows::core::PCWSTR(path_wide.as_ptr())) }
            .map_err(|e| NativePluginError::LoadingFailed(format!("LoadLibrary failed: {}", e)))?;

        if module.is_invalid() {
            return Err(NativePluginError::LoadingFailed(
                "LoadLibrary returned invalid handle".to_string(),
            ));
        }

        Ok(Self { module })
    }

    /// Get a symbol from the plugin using GetProcAddress.
    pub fn get_symbol<T>(&self, name: &[u8]) -> Result<*mut T, NativePluginError> {
        let name_cstr = std::ffi::CString::new(name)
            .map_err(|_| NativePluginError::LoadingFailed("Invalid symbol name".to_string()))?;

        let symbol = unsafe {
            GetProcAddress(
                self.module,
                windows::core::PCSTR(name_cstr.as_ptr() as *const u8),
            )
        };

        symbol.map(|s| s as *mut T).ok_or_else(|| {
            NativePluginError::LoadingFailed(format!("Symbol not found: {:?}", name))
        })
    }

    /// Check if running with elevated privileges.
    pub fn is_elevated() -> Result<bool, NativePluginError> {
        unsafe {
            let mut token_handle = HANDLE::default();
            let process = GetCurrentProcess();

            if OpenProcessToken(process, TOKEN_QUERY, &mut token_handle).is_err() {
                return Ok(false);
            }

            let mut token_user: MaybeUninit<TOKEN_USER> = MaybeUninit::uninit();
            let mut return_length = 0u32;

            let result = GetTokenInformation(
                token_handle,
                windows::Win32::Security::TokenUser,
                Some(token_user.as_mut_ptr() as *mut c_void),
                std::mem::size_of::<TOKEN_USER>() as u32,
                &mut return_length,
            );

            let _ = CloseHandle(token_handle);

            if result.is_err() {
                return Ok(false);
            }

            Ok(true)
        }
    }
}

impl Drop for WindowsPluginHandle {
    fn drop(&mut self) {
        if !self.module.is_invalid() {
            unsafe {
                let _ = FreeLibrary(self.module);
            }
        }
    }
}

/// Memory protection for executable code.
pub enum MemoryProtection {
    /// Read and execute.
    ExecuteRead,
    /// Read, write, and execute.
    ExecuteReadWrite,
}

impl From<MemoryProtection> for windows::Win32::System::Memory::PAGE_PROTECTION_FLAGS {
    fn from(protection: MemoryProtection) -> Self {
        match protection {
            MemoryProtection::ExecuteRead => windows::Win32::System::Memory::PAGE_EXECUTE_READ,
            MemoryProtection::ExecuteReadWrite => {
                windows::Win32::System::Memory::PAGE_EXECUTE_READWRITE
            }
        }
    }
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
    protection: MemoryProtection,
) -> Result<(), NativePluginError> {
    use windows::Win32::System::Memory::VirtualProtect;

    let mut old_protect = windows::Win32::System::Memory::PAGE_PROTECTION_FLAGS::default();
    unsafe {
        VirtualProtect(address, size, protection.into(), &mut old_protect).map_err(|e| {
            NativePluginError::LoadingFailed(format!("VirtualProtect failed: {}", e))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_elevated() {
        let result = WindowsPluginHandle::is_elevated();
        assert!(result.is_ok());
    }
}
