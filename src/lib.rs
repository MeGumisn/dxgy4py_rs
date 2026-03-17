pub mod captures;

use crate::captures::{GrabItem, start_grab, stop};
use std::sync::Arc;
use windows_capture::window::Window;
// --- 在原有代碼基礎上添加以下接口 ---

#[unsafe(no_mangle)]
pub extern "C" fn init_dxgi(window_handle: isize) -> *mut Arc<GrabItem> {
    // 通過句柄獲取 Window 對象
    let window = Window::from_raw_hwnd(window_handle as _);
    let handler_arc = start_grab(window);

    // 將 Arc 移入 Box 並轉換為原始指針返回給 Python
    Box::into_raw(Box::new(handler_arc))
}

#[unsafe(no_mangle)]
pub extern "C" fn grab(
    ptr: *mut Arc<GrabItem>,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    dst_buf: *mut u8,
) -> *mut u8 {
    let handler_arc = unsafe { &*ptr };
    let data = captures::grab(handler_arc.clone(), left, top, right, bottom, dst_buf);
    data as *mut u8
}

#[unsafe(no_mangle)]
pub extern "C" fn destroy(ptr: *mut Arc<GrabItem>) {
    if ptr.is_null() {
        return;
    }
    let handler_arc = unsafe { Box::from_raw(ptr) }; // 重新接管所有權
    stop(*handler_arc); // 函數結束後 Arc 會自動 drop
}

#[cfg(test)]
#[unsafe(no_mangle)]
pub extern "C" fn free_buffer(ptr: *mut u8, len: usize) {
    // 用於釋放 grab_c 分配的圖像內存
    if !ptr.is_null() {
        unsafe {
            let _ = Vec::from_raw_parts(ptr, len, len);
        }
    }
}
