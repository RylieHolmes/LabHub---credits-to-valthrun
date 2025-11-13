// controller/src/utils/mod.rs

use std::ffi::CString;

use windows::{
    core::PCSTR,
    Win32::{
        Foundation::HWND,
        UI::{
            Shell::ShellExecuteA,
            WindowsAndMessaging::SW_SHOW,
        },
    },
};

pub mod imgui;
pub use self::imgui::*;

mod console_io;
pub use console_io::*;

// ADDED: This line makes the code in `fs.rs` available to the rest of the project.
mod fs;
pub use fs::*;

#[allow(unused)]
pub fn open_url(url: &str) {
    unsafe {
        let url = match CString::new(url) {
            Ok(url) => url,
            Err(_) => return,
        };

        ShellExecuteA(
            HWND::default(),
            PCSTR::null(),
            PCSTR(url.as_bytes().as_ptr()),
            PCSTR::null(),
            PCSTR::null(),
            SW_SHOW,
        );
    }
}