use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::sleep;
use std::time::Duration;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
};


// 創建一個包裝類型
struct SafePtr(*mut u8);

// 手動實現 Send 和 Sync (這在 FFI 中很常見)
unsafe impl Send for SafePtr {}
unsafe impl Sync for SafePtr {}

pub struct GrabItem {
    #[cfg(test)]
    buffer: Mutex<Vec<u8>>,
    left: AtomicU32,
    top: AtomicU32,
    right: AtomicU32,
    bottom: AtomicU32,
    // 需要进行截屏操作
    should_capture: AtomicBool,
    capture_finished: AtomicBool,
    // 需要停止这个session
    should_stop: AtomicBool,
    stop_succeeded: AtomicBool,
    // python传过来的指针
    dst_buf: Mutex<SafePtr>,
}
struct CaptureHandler {
    grab_item: Arc<GrabItem>,
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = Arc<GrabItem>;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            grab_item: ctx.flags,
        })
    }

    // 當新幀到達時觸發
    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        // 有停止请求发送过来，停止
        if self.grab_item.should_stop.load(Ordering::Acquire) {
            eprintln!("stop capture");
            #[cfg(test)]
            {
                use windows_capture::frame::ImageFormat;
                let mut buffer = frame.buffer()?;
                let mut file_name = (self.grab_item.as_ref() as *const _ as usize).to_string();
                file_name.push_str(".png");
                buffer.save_as_image(file_name, ImageFormat::Png)?;
            }
            _capture_control.stop(); // 正確停止捕獲循環 [3, 4]
            return Ok(());
        }
        // 没有截图请求发送过来，空跑
        if !self.grab_item.should_capture.load(Ordering::Acquire) {
            return Ok(());
        }
        // 开始截图
        // 獲取原始像素數據 (Bgra8 格式)
        #[cfg(test)]
        println!("frame arrived");
        let grab_item = self.grab_item.clone();
        let left = grab_item.left.load(Ordering::Acquire);
        let top = grab_item.top.load(Ordering::Acquire);
        let right = grab_item.right.load(Ordering::Acquire);
        let bottom = grab_item.bottom.load(Ordering::Acquire);

        let mut frame_buffer = Frame::buffer_crop(frame, left, top, right, bottom)?;
        #[cfg(test)]
        println!(
            "frame cropped, left: {}, top: {}, right: {}, bottom: {}",
            left, top, right, bottom
        );

        // 這裡直接進行高效拷貝。Rust 編譯器會自動對此循環進行 SIMD (AVX2) 優化
        let data = frame_buffer.as_nopadding_buffer()?;
        #[cfg(test)]
        println!("frame as_no_padding_buffer");
        unsafe {
            // TODO 自定义ERROR
            let dst = self.grab_item.dst_buf.lock().unwrap();
            if dst.0.is_null() {
                eprintln!("Error: Target pointer is NULL");
                return Ok(());
            }
            #[cfg(test)]{
                let data_len = data.len();
                println!("Copying: src_len={}, dst_ptr={:?}", data_len, dst.0);
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst.0, data.len())
        }

        #[cfg(test)]
        {
            // TODO 自定义ERROR
            let mut buffer = grab_item.buffer.lock().unwrap();
            let data_len = data.len();
            println!(
                "frame start copy to buffer, buffer len:{}, data len: {}",
                buffer.len(),
                data_len
            );

            if buffer.len() != data_len {
                println!("new buffer created");
                *buffer = vec![0u8; data_len];
            }
            buffer.copy_from_slice(data);
        }
        #[cfg(test)]
        println!("frame copied");
        grab_item.capture_finished.store(true, Ordering::Release);
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        println!("Capture session closed");
        self.grab_item.stop_succeeded.store(true, Ordering::Release);
        Ok(())
    }
}

pub fn start_grab(window: Window) -> Arc<GrabItem> {
    let handler = GrabItem {
        #[cfg(test)]
        buffer: Mutex::new(Vec::new()),
        left: AtomicU32::new(0),
        top: AtomicU32::new(0),
        right: AtomicU32::new(100),
        bottom: AtomicU32::new(100),
        should_capture: AtomicBool::new(false),
        capture_finished: AtomicBool::new(true),
        should_stop: AtomicBool::new(false),
        stop_succeeded: AtomicBool::new(true),
        dst_buf: Mutex::new(SafePtr{0:null_mut()}),
    };
    let handler_arc = Arc::new(handler);
    // 2. 設定 (對應你設置 IsCursorCaptureEnabled 和 IsBorderRequired)
    let settings = Settings::new(
        window,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8, // 與你 C++ 代碼一致
        handler_arc.clone(),
    );

    // 開一個新線程跑捕獲，不要阻塞主線程
    thread::spawn(move || {
        if let Err(e) = CaptureHandler::start(settings) {
            eprintln!("Capture session failed to start: {:?}", e);
        }
    });
    eprintln!("capture session started, handler: {}", handler_arc.as_ref() as *const _ as usize);
    handler_arc
}

pub fn grab(
    handler_arc: Arc<GrabItem>,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    dst_buf: *mut u8,
) -> *const u8 {
    handler_arc.left.store(left, Ordering::Release);
    handler_arc.top.store(top, Ordering::Release);
    handler_arc.right.store(right, Ordering::Release);
    handler_arc.bottom.store(bottom, Ordering::Release);
    // 確保在設置 should_capture 之前，capture_finished 是 false
    handler_arc.capture_finished.store(false, Ordering::Release);
    {
        let mut dst = handler_arc.dst_buf.lock().unwrap();
        dst.0 = dst_buf;
    }
    handler_arc.should_capture.store(true, Ordering::Release);
    while !handler_arc.capture_finished.load(Ordering::Acquire) {
        thread::yield_now();
    }
    // 这里需要保证是顺序调用的，否则会读取到错误数据
    handler_arc.should_capture.store(false, Ordering::Release);
    dst_buf
}

pub fn stop(handler_arc: Arc<GrabItem>) {
    handler_arc.should_stop.store(true, Ordering::Release);
    while !handler_arc.stop_succeeded.load(Ordering::Acquire) {
        sleep(Duration::from_millis(1));
    }
}

// 模擬 grab() 函數調用
#[cfg(test)]
fn grab_monitor(handler_arc: Arc<GrabItem>) {
    let mut z = 0;
    loop {
        sleep(Duration::from_millis(16));
        let buffer = &handler_arc.buffer;
        let data = buffer.lock().unwrap();
        if !data.is_empty() {
            println!("Captured frame size: {} bytes", data.len());
            // 這裡可以根據 region (left, top, width, height) 進行切片
        }
        handler_arc.left.store(z, Ordering::Relaxed);
        handler_arc.top.store(z, Ordering::Relaxed);
        handler_arc.right.store(z + 100, Ordering::Relaxed);
        handler_arc.bottom.store(z + 100, Ordering::Relaxed);
        println!("z: {}", z);
        z += 10;
        if z % 50 == 0 {
            handler_arc.should_capture.store(true, Ordering::Relaxed);
        }
        if z > 500 {
            handler_arc.should_stop.store(true, Ordering::Relaxed);
            break;
        }
    }
}

#[cfg(test)]
pub fn test_grab() {
    // 1. 查找窗口 (根據標題，替代 Hwnd 手動查找)
    let window_qq = Window::enumerate()
        .expect("Failed to enumerate windows")
        .into_iter()
        .find(|w| w.title().expect("REASON").contains("QQ"))
        .expect("Window not found");

    let handler_arc_qq = start_grab(window_qq);

    let window_phantom = Window::enumerate()
        .expect("Failed to enumerate windows")
        .into_iter()
        .find(|w| w.title().expect("REASON").contains("PHAN"))
        .expect("Window not found");
    let handler_arc_phantom = start_grab(window_phantom);
    sleep(Duration::from_secs(5));

    thread::spawn(|| grab_monitor(handler_arc_phantom));
    grab_monitor(handler_arc_qq);

    sleep(Duration::from_secs(10));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_grab_frame() {
        test_grab();
    }
}
