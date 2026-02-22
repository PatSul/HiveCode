//! UI test runner - brings Hive to front, clicks sidebar panels, captures screenshots.
//! Uses Win32 APIs for window management and enigo for mouse input.
#![allow(unsafe_op_in_unsafe_fn, dead_code)]

use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};
use std::env;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr;
use std::thread;
use std::time::Duration;

// Win32 FFI
#[allow(non_snake_case)]
mod win32 {
    use std::ffi::c_void;

    #[repr(C)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    type HWND = *mut c_void;
    type BOOL = i32;
    type DWORD = u32;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> HWND;
        pub fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
        pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
        pub fn BringWindowToTop(hWnd: HWND) -> BOOL;
        pub fn GetWindowRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
        pub fn IsWindowVisible(hWnd: HWND) -> BOOL;
        pub fn GetForegroundWindow() -> HWND;
        pub fn GetWindowThreadProcessId(hWnd: HWND, lpdwProcessId: *mut DWORD) -> DWORD;
        pub fn AttachThreadInput(idAttach: DWORD, idAttachTo: DWORD, fAttach: BOOL) -> BOOL;
    }

    unsafe extern "system" {
        pub fn GetCurrentThreadId() -> DWORD;
    }

    pub const SW_RESTORE: i32 = 9;
    pub const SW_SHOW: i32 = 5;
    pub const SW_MAXIMIZE: i32 = 3;

    pub unsafe fn force_foreground(hwnd: HWND) {
        unsafe {
            let fg = GetForegroundWindow();
            let mut dummy: DWORD = 0;
            let fg_thread = GetWindowThreadProcessId(fg, &mut dummy);
            let cur_thread = GetCurrentThreadId();
            let target_thread = GetWindowThreadProcessId(hwnd, &mut dummy);

            AttachThreadInput(cur_thread, fg_thread, 1);
            AttachThreadInput(cur_thread, target_thread, 1);

            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
            BringWindowToTop(hwnd);
            ShowWindow(hwnd, SW_MAXIMIZE);

            AttachThreadInput(cur_thread, fg_thread, 0);
            AttachThreadInput(cur_thread, target_thread, 0);
        }
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn find_hive_window() -> *mut std::ffi::c_void {
    let class_name = to_wide("Zed::Window");
    unsafe { win32::FindWindowW(class_name.as_ptr(), ptr::null()) }
}

fn click_at(enigo: &mut Enigo, x: i32, y: i32) {
    enigo.move_mouse(x, y, Coordinate::Abs).expect("move failed");
    thread::sleep(Duration::from_millis(100));
    enigo.button(Button::Left, Direction::Click).expect("click failed");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Commands:");
        eprintln!("  focus           — bring Hive window to foreground");
        eprintln!("  click <x> <y>   — click at screen coordinates");
        eprintln!("  sidebar_test    — full sidebar panel walkthrough");
        eprintln!("  info            — show Hive window info");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "info" => {
            let hwnd = find_hive_window();
            if hwnd.is_null() {
                println!("Hive window (Zed::Window) not found!");
                std::process::exit(1);
            }
            let mut rect = win32::RECT { left: 0, top: 0, right: 0, bottom: 0 };
            unsafe {
                win32::GetWindowRect(hwnd, &mut rect);
                let visible = win32::IsWindowVisible(hwnd);
                println!("Hive Zed::Window");
                println!("  Handle: {:?}", hwnd);
                println!("  Visible: {}", visible != 0);
                println!("  Rect: ({},{}) to ({},{}) = {}x{}",
                    rect.left, rect.top, rect.right, rect.bottom,
                    rect.right - rect.left, rect.bottom - rect.top);
            }
        }
        "focus" => {
            let hwnd = find_hive_window();
            if hwnd.is_null() {
                println!("Hive window not found!");
                std::process::exit(1);
            }
            println!("Bringing Hive to foreground...");
            unsafe { win32::force_foreground(hwnd); }
            thread::sleep(Duration::from_millis(500));
            let fg = unsafe { win32::GetForegroundWindow() };
            println!("Foreground: {:?} (target: {:?}, match: {})", fg, hwnd, fg == hwnd);
        }
        "click" => {
            let x: i32 = args[2].parse().unwrap();
            let y: i32 = args[3].parse().unwrap();
            let mut enigo = Enigo::new(&Settings::default()).expect("enigo init");
            println!("Clicking at ({x}, {y})");
            click_at(&mut enigo, x, y);
        }
        "sidebar_test" => {
            let hwnd = find_hive_window();
            if hwnd.is_null() {
                println!("Hive window not found!");
                std::process::exit(1);
            }

            // Get window position for coordinate mapping
            let mut rect = win32::RECT { left: 0, top: 0, right: 0, bottom: 0 };
            unsafe { win32::GetWindowRect(hwnd, &mut rect); }
            println!("Window rect: ({},{}) to ({},{})",
                rect.left, rect.top, rect.right, rect.bottom);

            // Bring to front
            println!("Bringing Hive to foreground...");
            unsafe { win32::force_foreground(hwnd); }
            thread::sleep(Duration::from_millis(1000));

            let fg = unsafe { win32::GetForegroundWindow() };
            println!("Foreground match: {}\n", fg == hwnd);

            let mut enigo = Enigo::new(&Settings::default()).expect("enigo init");

            // Sidebar items - positions relative to window origin
            // Window top-left is at (rect.left, rect.top)
            // Sidebar items in the capture bitmap start at ~x=80, y varies
            // Screen coord = window_left + bitmap_x
            let wx = rect.left; // -7 typically for maximized
            let wy = rect.top;  // -7

            let sidebar_items = vec![
                ("Chat",       wx + 100, wy + 250),
                ("History",    wx + 100, wy + 298),
                ("Files",      wx + 100, wy + 346),
                ("Specs",      wx + 100, wy + 394),
                ("Agents",     wx + 100, wy + 470),
                ("Workflows",  wx + 100, wy + 518),
                ("Channels",   wx + 100, wy + 566),
                ("Kanban",     wx + 100, wy + 614),
                ("Git_Ops",    wx + 100, wy + 662),
                ("Skills",     wx + 100, wy + 710),
                ("Routing",    wx + 100, wy + 758),
                ("Models",     wx + 100, wy + 806),
                ("Learning",   wx + 100, wy + 854),
            ];

            println!("=== HIVE SIDEBAR UI TEST ===");
            println!("Testing {} sidebar panels...\n", sidebar_items.len());

            for (i, (name, x, y)) in sidebar_items.iter().enumerate() {
                print!("  [{:2}/{}] {:<12} @ ({},{})... ",
                    i + 1, sidebar_items.len(), name, x, y);
                click_at(&mut enigo, *x, *y);
                thread::sleep(Duration::from_millis(2000));
                println!("CLICKED");
            }

            println!("\n=== SIDEBAR TEST COMPLETE ===");
        }
        _ => eprintln!("Unknown command"),
    }
}
