use std::ffi::c_void;

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

const COMBINED_SESSION: i32 = 0;
const LEFT_BUTTON: u32 = 0;
const CONTROL: u64 = 0x40000;
const OPTION: u64 = 0x80000;
const NONACTIVATING_PANEL: usize = 1 << 7;
const SCREEN_SAVER_LEVEL: isize = 1000;
const ALL_SPACES: usize = 1 << 0;
const STATIONARY: usize = 1 << 4;
const IGNORES_CYCLE: usize = 1 << 6;
const BESIDE_FULL_SCREEN: usize = 1 << 8;
const PANEL_CLASS: &str = "SthPlexPipPanel";

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

unsafe extern "C" {
    fn object_setClass(object: *mut AnyObject, class: *const AnyClass) -> *const AnyClass;
}

extern "C" fn yes(_: &AnyObject, _: Sel) -> Bool {
    Bool::YES
}

extern "C" fn no(_: &AnyObject, _: Sel) -> Bool {
    Bool::NO
}

fn build_panel_class() -> Option<&'static AnyClass> {
    let mut builder = ClassBuilder::new(PANEL_CLASS, class!(NSPanel))?;
    unsafe {
        builder.add_method(sel!(canBecomeKeyWindow), yes as extern "C" fn(&AnyObject, Sel) -> Bool);
        builder.add_method(sel!(canBecomeMainWindow), no as extern "C" fn(&AnyObject, Sel) -> Bool);
    }
    Some(builder.register())
}

fn panel_class() -> Option<&'static AnyClass> {
    AnyClass::get(PANEL_CLASS).or_else(build_panel_class)
}

fn ns_window(window: &Window) -> Option<*mut AnyObject> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return None };
    let view = handle.ns_view.as_ptr() as *mut AnyObject;
    let ns_window: *mut AnyObject = unsafe { msg_send![view, window] };
    (!ns_window.is_null()).then_some(ns_window)
}

pub fn prepare(window: &Window) {
    let (Some(ns_window), Some(panel)) = (ns_window(window), panel_class()) else { return };
    unsafe {
        object_setClass(ns_window, panel);
        let mask: usize = msg_send![ns_window, styleMask];
        let _: () = msg_send![ns_window, setStyleMask: mask | NONACTIVATING_PANEL];
        let _: () = msg_send![ns_window, setLevel: SCREEN_SAVER_LEVEL];
        let behavior = ALL_SPACES | STATIONARY | IGNORES_CYCLE | BESIDE_FULL_SCREEN;
        let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        let _: () = msg_send![ns_window, setHidesOnDeactivate: false];
        let _: () = msg_send![ns_window, setFloatingPanel: true];
        let _: () = msg_send![ns_window, setBecomesKeyOnlyIfNeeded: true];
        let _: () = msg_send![ns_window, orderFrontRegardless];
    }
}

pub fn set_opacity(window: &Window, opacity: f32) {
    let Some(ns_window) = ns_window(window) else { return };
    let _: () = unsafe { msg_send![ns_window, setAlphaValue: opacity as f64] };
}

pub fn set_takes_keys(window: &Window, takes: bool) {
    let Some(ns_window) = ns_window(window) else { return };
    unsafe {
        let _: () = msg_send![ns_window, setBecomesKeyOnlyIfNeeded: !takes];
        if takes {
            let _: () = msg_send![ns_window, makeKeyWindow];
        }
    }
}

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
