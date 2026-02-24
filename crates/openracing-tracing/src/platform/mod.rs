//! Platform-specific tracing providers

mod fallback;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

pub use fallback::FallbackProvider;

#[cfg(target_os = "windows")]
pub use windows::WindowsETWProvider;

#[cfg(target_os = "linux")]
pub use linux::LinuxTracepointsProvider;
