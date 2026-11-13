use std::ffi::{CString, c_void};

use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
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
const SORTED_KEYS: usize = 1 << 1;
const BUNDLE_ID: &str = "com.sth.plexpip";

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

fn ns_string(text: &str) -> *mut AnyObject {
    let text = CString::new(text).unwrap_or_default();
    unsafe { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] }
}

fn timestamp() -> *mut AnyObject {
    unsafe {
        let formatter: *mut AnyObject = msg_send![class!(NSISO8601DateFormatter), new];
        let now: *mut AnyObject = msg_send![class!(NSDate), date];
        let stamp: *mut AnyObject = msg_send![formatter, stringFromDate: now];
        let _: () = msg_send![formatter, release];
        stamp
    }
}

fn state_file() -> Option<*mut AnyObject> {
    let folder = dirs::config_dir()?.join(BUNDLE_ID);
    std::fs::create_dir_all(&folder).ok()?;
    Some(ns_string(folder.join("state.json").to_str()?))
}

pub fn announce(state: &str) {
    unsafe {
        let fields: *mut AnyObject = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![fields, setObject: ns_string(state), forKey: ns_string("state")];
        let _: () = msg_send![fields, setObject: ns_string(&std::process::id().to_string()), forKey: ns_string("pid")];
        let _: () = msg_send![fields, setObject: timestamp(), forKey: ns_string("updatedAt")];
        let no_error = std::ptr::null_mut::<*mut AnyObject>();
        let json: *mut AnyObject =
            msg_send![class!(NSJSONSerialization), dataWithJSONObject: fields, options: SORTED_KEYS, error: no_error];
        if let (false, Some(file)) = (json.is_null(), state_file()) {
            let _: bool = msg_send![json, writeToFile: file, atomically: true];
        }
        let center: *mut AnyObject = msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
        let name = ns_string(&format!("{BUNDLE_ID}.state"));
        let nobody = std::ptr::null_mut::<AnyObject>();
        let _: () =
            msg_send![center, postNotificationName: name, object: nobody, userInfo: fields, deliverImmediately: true];
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
