use std::ffi::c_void;

use objc2::msg_send;
use objc2::runtime::AnyObject;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

const COMBINED_SESSION: i32 = 0;
const LEFT_BUTTON: u32 = 0;
const CONTROL: u64 = 0x40000;
const OPTION: u64 = 0x80000;
const SCREEN_SAVER_LEVEL: isize = 1000;
const ALL_SPACES: usize = 1 << 0;
const STATIONARY: usize = 1 << 4;
const IGNORES_CYCLE: usize = 1 << 6;
const BESIDE_FULL_SCREEN: usize = 1 << 8;

#[repr(C)]
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *mut c_void) -> Point;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventSourceFlagsState(state: i32) -> u64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: *const c_void);
}

fn ns_window(window: &Window) -> Option<*mut AnyObject> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
    let view = handle.ns_view.as_ptr() as *mut AnyObject;
    let ns_window: *mut AnyObject = unsafe { msg_send![view, window] };
    (!ns_window.is_null()).then_some(ns_window)
}

pub fn prepare(window: &Window) {
    let Some(ns_window) = ns_window(window) else { return };
    unsafe {
        let _: () = msg_send![ns_window, setLevel: SCREEN_SAVER_LEVEL];
        let behavior = ALL_SPACES | STATIONARY | IGNORES_CYCLE | BESIDE_FULL_SCREEN;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        let _: () = msg_send![ns_window, setHidesOnDeactivate: false];
        let _: () = msg_send![ns_window, orderFrontRegardless];
    }
}

pub fn set_opacity(window: &Window, opacity: f32) {
    let Some(ns_window) = ns_window(window) else { return };
    let _: () = unsafe { msg_send![ns_window, setAlphaValue: opacity as f64] };
}

pub fn set_takes_keys(_: &Window, _: bool) {}

pub fn cursor(_scale: f64) -> Option<(f32, f32)> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        Some((point.x as f32, point.y as f32))
    }
}

pub fn primary_button_down() -> bool {
    unsafe { CGEventSourceButtonState(COMBINED_SESSION, LEFT_BUTTON) }
}

pub fn modifier_held() -> bool {
    let flags = unsafe { CGEventSourceFlagsState(COMBINED_SESSION) };
    flags & CONTROL != 0 && flags & OPTION != 0
}
