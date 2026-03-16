use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
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

pub struct GrabItem {
    buffer: Mutex<Vec<u8>>,
    left: AtomicU32,
    top: AtomicU32,
    right: AtomicU32,
    bottom: AtomicU32,
    // 需要进行截屏操作
    should_capture: AtomicBool,
    // 需要停止这个session
    should_stop: AtomicBool,
}
// 用於存儲捕獲到的數據，模擬你的 SimpleDXGI 緩存
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

    // 當新幀到達時觸發 (對應你的 FrameArrived)
    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        // 有停止请求发送过来，停止
        if self.grab_item.should_stop.load(Ordering::Relaxed) {
            _capture_control.stop(); // 正確停止捕獲循環 [3, 4]
            return Ok(());
        }
        // 没有截图请求发送过来，空跑
        if !self.grab_item.should_capture.load(Ordering::Relaxed) {
            return Ok(());
        }
        // 开始截图了, 重新置为false
        self.grab_item
            .should_capture
            .store(false, Ordering::Relaxed);

        // 獲取原始像素數據 (BGRA8 格式)
        println!("frame arrived");
        let grab_item = self.grab_item.clone();
        let left = grab_item.left.load(Ordering::Relaxed);
        let top = grab_item.top.load(Ordering::Relaxed);
        let right = grab_item.right.load(Ordering::Relaxed);
        let bottom = grab_item.bottom.load(Ordering::Relaxed);
        println!("left: {}, top: {}", left, top);

        let mut frame_buffer = Frame::buffer_crop(frame, left, top, right, bottom)?;
        println!("frame cropped");

        // let mut frame_buffer = captured_frame.as_nopadding_buffer()?;

        // 這裡直接進行高效拷貝。Rust 編譯器會自動對此循環進行 SIMD (AVX2) 優化
        let data = frame_buffer.as_nopadding_buffer()?;
        let mut buffer = grab_item.buffer.lock().unwrap();
        let data_len = data.len();
        if buffer.len() != data_len {
            println!("new buffer created");
            *buffer = vec![0u8; data_len];
        }
        buffer.copy_from_slice(data);

        // #[cfg(test)]
        {
            use opencv::core::{Mat, Vec4b};
            use opencv::highgui::destroy_all_windows;
            use opencv::highgui::{imshow, wait_key};
            let width = right - left;
            let height = bottom - top;
            let captured_mat = Mat::new_rows_cols_with_bytes::<Vec4b>(
                height as i32,
                width as i32,
                buffer.as_slice(),
            )?;
            imshow("tests", &captured_mat)?;
            wait_key(0)?;
            // 這裡放你想在 ESC 後執行的代碼
            destroy_all_windows()?;
        }
        println!("frame copied");
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        println!("Capture session closed");
        Ok(())
    }
}

pub fn start_grab(window: Window) -> Arc<GrabItem> {
    let handler = GrabItem {
        buffer: Mutex::new(Vec::new()),
        left: AtomicU32::new(0),
        top: AtomicU32::new(0),
        right: AtomicU32::new(100),
        bottom: AtomicU32::new(100),
        should_capture: AtomicBool::new(true),
        should_stop: AtomicBool::new(false),
    };
    let handler_arc = Arc::new(handler);
    // 2. 設定 (對應你設置 IsCursorCaptureEnabled 和 IsBorderRequired)
    let settings = Settings::new(
        window,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8, // 與你 C++ 代碼一致
        handler_arc.clone(),
    );

    // 開一個新線程跑捕獲，不要阻塞主線程
    std::thread::spawn(move || {
        CaptureHandler::start(settings).expect("Capture failed");
    });
    return handler_arc;
}

pub fn test_grab(){
    // 1. 查找窗口 (根據標題，替代 HWND 手動查找)
    let window = Window::enumerate()
        .expect("Failed to enumerate windows")
        .into_iter()
        .find(|w| w.title().expect("REASON").contains("QQ"))
        .expect("Window not found");

    let handler_arc = start_grab(window);
    sleep(Duration::from_secs(5));

    // 模擬 grab() 函數調用
    let mut z = 0;
    loop {
        sleep(std::time::Duration::from_millis(16));
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
