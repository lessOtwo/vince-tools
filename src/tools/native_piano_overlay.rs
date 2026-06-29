#[cfg(target_os = "windows")]
mod platform {
    use std::{
        ffi::c_void,
        mem::{size_of, zeroed},
        ptr::{null, null_mut},
        sync::OnceLock,
    };

    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM},
        Graphics::Gdi::{
            AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
            CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
            HGDIOBJ, ReleaseDC, SelectObject,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
            GetSystemMetrics, HTTRANSPARENT, HWND_TOPMOST, IDC_ARROW, LoadCursorW, RegisterClassW,
            SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
            SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_NCHITTEST, WNDCLASSW,
            WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
            WS_POPUP,
        },
    };

    const CLASS_NAME: &str = "VinceToolsNativePianoOverlay";
    const WINDOW_TITLE: &str = "Vince Tools - Native Piano Overlay";

    #[derive(Clone, Copy)]
    pub enum OverlayKeyKind {
        White,
        Black,
    }

    #[derive(Clone)]
    pub struct OverlayKeyFrame {
        pub label: &'static str,
        pub x: f32,
        pub y: f32,
        pub width: f32,
        pub height: f32,
        pub active: f32,
        pub kind: OverlayKeyKind,
    }

    pub struct OverlayFrame {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
        pub opacity: f32,
        pub keys: Vec<OverlayKeyFrame>,
    }

    pub struct NativePianoOverlay {
        hwnd: HWND,
        buffer: Vec<u8>,
        last_size: (i32, i32),
    }

    impl NativePianoOverlay {
        pub fn new() -> Self {
            Self {
                hwnd: null_mut(),
                buffer: Vec::new(),
                last_size: (0, 0),
            }
        }

        pub fn primary_screen_size() -> (i32, i32) {
            unsafe {
                (
                    GetSystemMetrics(SM_CXSCREEN).max(1),
                    GetSystemMetrics(SM_CYSCREEN).max(1),
                )
            }
        }

        pub fn update(&mut self, frame: &OverlayFrame) {
            if frame.width <= 0 || frame.height <= 0 || frame.opacity <= 0.001 {
                self.hide();
                return;
            }

            if self.ensure_window().is_err() {
                return;
            }

            render_frame(frame, &mut self.buffer);
            self.last_size = (frame.width, frame.height);
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    frame.x,
                    frame.y,
                    frame.width,
                    frame.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ = update_layered_window(self.hwnd, frame, &self.buffer);
            }
        }

        pub fn hide(&mut self) {
            if !self.hwnd.is_null() {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
        }

        fn ensure_window(&mut self) -> Result<(), ()> {
            if !self.hwnd.is_null() {
                return Ok(());
            }

            register_window_class();
            let class_name = wide_static(CLASS_NAME);
            let title = wide_static(WINDOW_TITLE);

            unsafe {
                let instance = GetModuleHandleW(null());
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED
                        | WS_EX_TRANSPARENT
                        | WS_EX_TOPMOST
                        | WS_EX_TOOLWINDOW
                        | WS_EX_NOACTIVATE,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    null_mut(),
                    null_mut(),
                    instance,
                    null(),
                );

                if hwnd.is_null() {
                    return Err(());
                }

                self.hwnd = hwnd;
                Ok(())
            }
        }
    }

    impl Drop for NativePianoOverlay {
        fn drop(&mut self) {
            if !self.hwnd.is_null() {
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
                self.hwnd = null_mut();
            }
        }
    }

    fn update_layered_window(hwnd: HWND, frame: &OverlayFrame, pixels: &[u8]) -> bool {
        unsafe {
            let screen_dc = GetDC(null_mut());
            if screen_dc.is_null() {
                return false;
            }

            let mem_dc = CreateCompatibleDC(screen_dc);
            if mem_dc.is_null() {
                let _ = ReleaseDC(null_mut(), screen_dc);
                return false;
            }

            let mut bits: *mut c_void = null_mut();
            let bitmap_info = bitmap_info(frame.width, frame.height);
            let bitmap = CreateDIBSection(
                mem_dc,
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            );

            if bitmap.is_null() || bits.is_null() {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(null_mut(), screen_dc);
                return false;
            }

            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());

            let old = SelectObject(mem_dc, bitmap as HGDIOBJ);
            let dst = POINT {
                x: frame.x,
                y: frame.y,
            };
            let size = SIZE {
                cx: frame.width,
                cy: frame.height,
            };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let ok = UpdateLayeredWindow(
                hwnd, screen_dc, &dst, &size, mem_dc, &src, 0, &blend, ULW_ALPHA,
            ) != 0;

            if !old.is_null() {
                let _ = SelectObject(mem_dc, old);
            }
            let _ = DeleteObject(bitmap as HGDIOBJ);
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(null_mut(), screen_dc);

            ok
        }
    }

    fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
        let mut info: BITMAPINFO = unsafe { zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: (width * height * 4).max(0) as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        info
    }

    fn register_window_class() {
        static REGISTERED: OnceLock<()> = OnceLock::new();
        REGISTERED.get_or_init(|| unsafe {
            let class_name = wide_static(CLASS_NAME);
            let instance = GetModuleHandleW(null());
            let wnd_class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(overlay_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: null_mut(),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: null_mut(),
                lpszMenuName: null(),
                lpszClassName: class_name.as_ptr(),
            };
            let _ = RegisterClassW(&wnd_class);
        });
    }

    unsafe extern "system" fn overlay_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_NCHITTEST {
            return HTTRANSPARENT as LRESULT;
        }

        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn wide_static(text: &'static str) -> &'static [u16] {
        static CLASS: OnceLock<Vec<u16>> = OnceLock::new();
        static TITLE: OnceLock<Vec<u16>> = OnceLock::new();

        let value = if text == CLASS_NAME {
            CLASS.get_or_init(|| wide_null(text))
        } else {
            TITLE.get_or_init(|| wide_null(text))
        };
        value.as_slice()
    }

    fn wide_null(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn render_frame(frame: &OverlayFrame, buffer: &mut Vec<u8>) {
        let width = frame.width.max(1) as usize;
        let height = frame.height.max(1) as usize;
        let needed = width * height * 4;
        if buffer.len() != needed {
            buffer.resize(needed, 0);
        } else {
            buffer.fill(0);
        }

        let opacity = frame.opacity.clamp(0.0, 0.5);

        for key in &frame.keys {
            draw_key(buffer, width, height, key, opacity);
        }
    }

    fn draw_key(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        key: &OverlayKeyFrame,
        opacity: f32,
    ) {
        let rect = FloatRect {
            x: key.x,
            y: key.y + key.active * 7.0,
            width: key.width,
            height: key.height,
        };
        let accent = match key.kind {
            OverlayKeyKind::White => Rgba::rgb(252, 211, 77),
            OverlayKeyKind::Black => Rgba::rgb(129, 140, 248),
        };

        if key.active > 0.0 {
            let glow_alpha = (150.0 * key.active * opacity).round().clamp(0.0, 255.0) as u8;
            draw_rounded_rect(
                buffer,
                width,
                height,
                rect.expand(10.0 + key.active * 12.0),
                12.0,
                Rgba::rgba(accent.r, accent.g, accent.b, glow_alpha),
            );
            let stroke_alpha = (220.0 * key.active * opacity).round().clamp(0.0, 255.0) as u8;
            draw_rounded_stroke(
                buffer,
                width,
                height,
                rect.expand(4.0 + key.active * 4.0),
                10.0,
                2.0,
                Rgba::rgba(accent.r, accent.g, accent.b, stroke_alpha),
            );
        }

        let shadow = FloatRect {
            x: rect.x + 2.0,
            y: rect.y + 5.0,
            width: rect.width,
            height: rect.height + 3.0,
        };
        draw_rounded_rect(
            buffer,
            width,
            height,
            shadow,
            8.0,
            with_opacity(Rgba::rgba(0, 0, 0, 92), opacity),
        );

        let (top, bottom, text, stroke) = key_palette(key.kind, key.active, opacity);
        draw_rounded_vertical_gradient(buffer, width, height, rect, 8.0, top, bottom);
        draw_rounded_stroke(buffer, width, height, rect, 8.0, 1.0, stroke);

        let label = display_label(key.label);
        let scale = if label.len() > 1 { 1 } else { 2 };
        let label_y = match key.kind {
            OverlayKeyKind::White => rect.y + rect.height - 28.0,
            OverlayKeyKind::Black => rect.y + rect.height - 22.0,
        };
        draw_text_centered(
            buffer,
            width,
            height,
            label,
            rect.x + rect.width / 2.0,
            label_y,
            scale,
            text,
        );
    }

    fn key_palette(kind: OverlayKeyKind, active: f32, opacity: f32) -> (Rgba, Rgba, Rgba, Rgba) {
        let accent = match kind {
            OverlayKeyKind::White => Rgba::rgb(252, 211, 77),
            OverlayKeyKind::Black => Rgba::rgb(129, 140, 248),
        };
        match kind {
            OverlayKeyKind::White => (
                with_opacity(
                    mix(Rgba::rgb(255, 255, 255), accent, active * 0.18),
                    opacity,
                ),
                with_opacity(
                    mix(Rgba::rgb(255, 255, 255), accent, active * 0.12),
                    opacity,
                ),
                with_opacity(Rgba::rgb(15, 23, 42), opacity),
                with_opacity(Rgba::rgba(255, 255, 255, 180), opacity),
            ),
            OverlayKeyKind::Black => (
                with_opacity(
                    mix(Rgba::rgba(17, 24, 39, 238), accent, active * 0.55),
                    opacity,
                ),
                with_opacity(
                    mix(Rgba::rgba(2, 6, 23, 246), accent, active * 0.35),
                    opacity,
                ),
                with_opacity(Rgba::rgb(226, 232, 240), opacity),
                with_opacity(Rgba::rgba(255, 255, 255, 44), opacity),
            ),
        }
    }

    fn display_label(label: &'static str) -> &'static str {
        match label {
            "Backspace" => "Bksp",
            "Enter" => "Ent",
            other => other,
        }
    }

    #[derive(Clone, Copy)]
    struct FloatRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    }

    impl FloatRect {
        fn right(self) -> f32 {
            self.x + self.width
        }

        fn bottom(self) -> f32 {
            self.y + self.height
        }

        fn expand(self, amount: f32) -> Self {
            Self {
                x: self.x - amount,
                y: self.y - amount,
                width: self.width + amount * 2.0,
                height: self.height + amount * 2.0,
            }
        }

        fn shrink(self, amount: f32) -> Self {
            Self {
                x: self.x + amount,
                y: self.y + amount,
                width: (self.width - amount * 2.0).max(0.0),
                height: (self.height - amount * 2.0).max(0.0),
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Rgba {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    }

    impl Rgba {
        fn rgb(r: u8, g: u8, b: u8) -> Self {
            Self { r, g, b, a: 255 }
        }

        fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
            Self { r, g, b, a }
        }
    }

    fn with_opacity(color: Rgba, opacity: f32) -> Rgba {
        Rgba {
            a: (color.a as f32 * opacity).round().clamp(0.0, 255.0) as u8,
            ..color
        }
    }

    fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
        let t = t.clamp(0.0, 1.0);
        let lerp = |left: u8, right: u8| -> u8 {
            (left as f32 + (right as f32 - left as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgba::rgba(
            lerp(a.r, b.r),
            lerp(a.g, b.g),
            lerp(a.b, b.b),
            lerp(a.a, b.a),
        )
    }

    fn draw_rounded_rect(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        rect: FloatRect,
        radius: f32,
        color: Rgba,
    ) {
        draw_rounded(buffer, width, height, rect, radius, |_, _| color);
    }

    fn draw_rounded_vertical_gradient(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        rect: FloatRect,
        radius: f32,
        top: Rgba,
        bottom: Rgba,
    ) {
        draw_rounded(buffer, width, height, rect, radius, |_, y| {
            let t = ((y as f32 + 0.5 - rect.y) / rect.height.max(1.0)).clamp(0.0, 1.0);
            mix(top, bottom, t)
        });
    }

    fn draw_rounded_stroke(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        rect: FloatRect,
        radius: f32,
        thickness: f32,
        color: Rgba,
    ) {
        let inner = rect.shrink(thickness);
        let inner_radius = (radius - thickness).max(0.0);
        let x0 = rect.x.floor().max(0.0) as i32;
        let y0 = rect.y.floor().max(0.0) as i32;
        let x1 = rect.right().ceil().min(width as f32) as i32;
        let y1 = rect.bottom().ceil().min(height as f32) as i32;

        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                if inside_rounded(rect, radius, px, py)
                    && !inside_rounded(inner, inner_radius, px, py)
                {
                    blend_pixel(buffer, width, x as usize, y as usize, color);
                }
            }
        }
    }

    fn draw_rounded(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        rect: FloatRect,
        radius: f32,
        mut color_at: impl FnMut(i32, i32) -> Rgba,
    ) {
        let x0 = rect.x.floor().max(0.0) as i32;
        let y0 = rect.y.floor().max(0.0) as i32;
        let x1 = rect.right().ceil().min(width as f32) as i32;
        let y1 = rect.bottom().ceil().min(height as f32) as i32;

        for y in y0..y1 {
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                if inside_rounded(rect, radius, px, py) {
                    blend_pixel(buffer, width, x as usize, y as usize, color_at(x, y));
                }
            }
        }
    }

    fn inside_rounded(rect: FloatRect, radius: f32, px: f32, py: f32) -> bool {
        if px < rect.x || px >= rect.right() || py < rect.y || py >= rect.bottom() {
            return false;
        }
        let radius = radius.max(0.0).min(rect.width * 0.5).min(rect.height * 0.5);
        if radius <= 0.0 {
            return true;
        }

        let cx = if px < rect.x + radius {
            rect.x + radius
        } else if px > rect.right() - radius {
            rect.right() - radius
        } else {
            px
        };
        let cy = if py < rect.y + radius {
            rect.y + radius
        } else if py > rect.bottom() - radius {
            rect.bottom() - radius
        } else {
            py
        };
        let dx = px - cx;
        let dy = py - cy;
        dx * dx + dy * dy <= radius * radius
    }

    fn blend_pixel(buffer: &mut [u8], width: usize, x: usize, y: usize, color: Rgba) {
        if color.a == 0 {
            return;
        }

        let index = (y * width + x) * 4;
        let inv = 255u16.saturating_sub(color.a as u16);
        let src_a = color.a as u16;
        let src_r = color.r as u16 * src_a / 255;
        let src_g = color.g as u16 * src_a / 255;
        let src_b = color.b as u16 * src_a / 255;

        let dst_b = buffer[index] as u16;
        let dst_g = buffer[index + 1] as u16;
        let dst_r = buffer[index + 2] as u16;
        let dst_a = buffer[index + 3] as u16;

        buffer[index] = (src_b + dst_b * inv / 255).min(255) as u8;
        buffer[index + 1] = (src_g + dst_g * inv / 255).min(255) as u8;
        buffer[index + 2] = (src_r + dst_r * inv / 255).min(255) as u8;
        buffer[index + 3] = (src_a + dst_a * inv / 255).min(255) as u8;
    }

    fn draw_text_centered(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        text: &str,
        center_x: f32,
        y: f32,
        scale: i32,
        color: Rgba,
    ) {
        let text_width = measure_text(text, scale);
        let mut x = (center_x - text_width as f32 / 2.0).round() as i32;
        let y = y.round() as i32;

        for ch in text.chars() {
            let (rows, glyph_width) = glyph(ch.to_ascii_uppercase());
            for row in 0..7 {
                for col in 0..glyph_width {
                    let mask = 1 << (glyph_width - 1 - col);
                    if rows[row as usize] & mask == 0 {
                        continue;
                    }
                    draw_text_pixel(
                        buffer,
                        width,
                        height,
                        x + col * scale,
                        y + row * scale,
                        scale,
                        color,
                    );
                }
            }
            x += (glyph_width + 1) * scale;
        }
    }

    fn draw_text_pixel(
        buffer: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        scale: i32,
        color: Rgba,
    ) {
        for py in y..y + scale {
            for px in x..x + scale {
                if px >= 0 && py >= 0 && (px as usize) < width && (py as usize) < height {
                    blend_pixel(buffer, width, px as usize, py as usize, color);
                }
            }
        }
    }

    fn measure_text(text: &str, scale: i32) -> i32 {
        text.chars()
            .map(|ch| {
                let (_, width) = glyph(ch.to_ascii_uppercase());
                (width + 1) * scale
            })
            .sum::<i32>()
            .saturating_sub(scale)
    }

    fn glyph(ch: char) -> ([u8; 7], i32) {
        match ch {
            'A' => (
                [
                    0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
                ],
                5,
            ),
            'B' => (
                [
                    0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
                ],
                5,
            ),
            'C' => (
                [
                    0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
                ],
                5,
            ),
            'D' => (
                [
                    0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
                ],
                5,
            ),
            'E' => (
                [
                    0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
                ],
                5,
            ),
            'F' => (
                [
                    0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
                ],
                5,
            ),
            'G' => (
                [
                    0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
                ],
                5,
            ),
            'H' => (
                [
                    0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
                ],
                5,
            ),
            'I' => (
                [
                    0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
                ],
                5,
            ),
            'J' => (
                [
                    0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
                ],
                5,
            ),
            'K' => (
                [
                    0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
                ],
                5,
            ),
            'L' => (
                [
                    0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
                ],
                5,
            ),
            'M' => (
                [
                    0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
                ],
                5,
            ),
            'N' => (
                [
                    0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
                ],
                5,
            ),
            'O' => (
                [
                    0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
                ],
                5,
            ),
            'P' => (
                [
                    0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
                ],
                5,
            ),
            'Q' => (
                [
                    0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
                ],
                5,
            ),
            'R' => (
                [
                    0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
                ],
                5,
            ),
            'S' => (
                [
                    0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
                ],
                5,
            ),
            'T' => (
                [
                    0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
                ],
                5,
            ),
            'U' => (
                [
                    0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
                ],
                5,
            ),
            'V' => (
                [
                    0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
                ],
                5,
            ),
            'W' => (
                [
                    0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
                ],
                5,
            ),
            'X' => (
                [
                    0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
                ],
                5,
            ),
            'Y' => (
                [
                    0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
                ],
                5,
            ),
            'Z' => (
                [
                    0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
                ],
                5,
            ),
            ' ' => ([0; 7], 3),
            _ => (
                [
                    0b11111, 0b00001, 0b00010, 0b00100, 0b00100, 0b00000, 0b00100,
                ],
                5,
            ),
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    #[derive(Clone, Copy)]
    pub enum OverlayKeyKind {
        White,
        Black,
    }

    #[derive(Clone)]
    pub struct OverlayKeyFrame {
        pub label: &'static str,
        pub x: f32,
        pub y: f32,
        pub width: f32,
        pub height: f32,
        pub active: f32,
        pub kind: OverlayKeyKind,
    }

    pub struct OverlayFrame {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
        pub opacity: f32,
        pub keys: Vec<OverlayKeyFrame>,
    }

    pub struct NativePianoOverlay;

    impl NativePianoOverlay {
        pub fn new() -> Self {
            Self
        }

        pub fn primary_screen_size() -> (i32, i32) {
            (1280, 720)
        }

        pub fn update(&mut self, _frame: &OverlayFrame) {}

        pub fn hide(&mut self) {}
    }
}

pub use platform::{NativePianoOverlay, OverlayFrame, OverlayKeyFrame, OverlayKeyKind};
