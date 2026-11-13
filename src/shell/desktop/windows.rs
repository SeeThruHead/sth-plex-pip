use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LBUTTON, VK_MENU};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

fn hwnd(window: &Window) -> Option<HWND> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return None };
    Some(handle.hwnd.get() as HWND)
}

fn with_style(hwnd: HWND, add: u32, remove: u32) {
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ((style | add) & !remove) as isize);
    }
}

fn keep_on_top(hwnd: HWND) {
    unsafe {
        SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

pub fn prepare(window: &Window) {
    let Some(hwnd) = hwnd(window) else { return };
    with_style(hwnd, WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE, 0);
    keep_on_top(hwnd);
}

pub fn set_opacity(window: &Window, opacity: f32) {
    let Some(hwnd) = hwnd(window) else { return };
    with_style(hwnd, WS_EX_LAYERED, 0);
    unsafe {
        SetLayeredWindowAttributes(hwnd, 0, (opacity.clamp(0.0, 1.0) * 255.0) as u8, LWA_ALPHA);
    }
    keep_on_top(hwnd);
}

pub fn set_takes_keys(window: &Window, takes: bool) {
    let Some(hwnd) = hwnd(window) else { return };
    if takes {
        with_style(hwnd, 0, WS_EX_NOACTIVATE);
        window.focus_window();
        return;
    }
    with_style(hwnd, WS_EX_NOACTIVATE, 0);
}

pub fn cursor(scale: f64) -> Option<(f32, f32)> {
    let mut point = POINT { x: 0, y: 0 };
    let found = unsafe { GetCursorPos(&mut point) } != 0;
    found.then_some(((point.x as f64 / scale) as f32, (point.y as f64 / scale) as f32))
}

fn down(key: u16) -> bool {
    (unsafe { GetAsyncKeyState(key as i32) } as u16) & 0x8000 != 0
}

pub fn primary_button_down() -> bool {
    down(VK_LBUTTON)
}

pub fn modifier_held() -> bool {
    down(VK_CONTROL) && down(VK_MENU)
}
