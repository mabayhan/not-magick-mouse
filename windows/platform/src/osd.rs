//! Plain on-screen text: a borderless, topmost, click-through window that is
//! shown and then hidden by a timer. No animation.

use crate::util::wide;
use std::cell::RefCell;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

const TIMER_ID: usize = 1;

thread_local! {
    static TEXT: RefCell<String> = const { RefCell::new(String::new()) };
}

pub struct Osd {
    hwnd: HWND,
}

impl Osd {
    pub fn new() -> windows::core::Result<Osd> {
        unsafe {
            let hinst = HINSTANCE(GetModuleHandleW(None)?.0);
            let class = wide("NmmOsd");
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: hinst,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(hinst),
                None,
            )?;
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA)?;
            Ok(Osd { hwnd })
        }
    }

    /// `position` is one of top/center/bottom + left/center/right, e.g. `bottom-center`.
    pub fn show(&self, text: &str, millis: u32, position: &str) {
        TEXT.with(|t| *t.borrow_mut() = text.to_string());
        unsafe {
            let font = make_font();
            let hdc = GetDC(Some(self.hwnd));
            let old = SelectObject(hdc, font.into());
            let w = wide(text);
            let mut size = SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, &w[..w.len() - 1], &mut size);
            SelectObject(hdc, old);
            ReleaseDC(Some(self.hwnd), hdc);
            let _ = DeleteObject(font.into());

            let (width, height) = (size.cx + 64, size.cy + 32);

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let r = mi.rcWork;
            let margin = 48;
            let x = if position.ends_with("left") {
                r.left + margin
            } else if position.ends_with("right") {
                r.right - width - margin
            } else {
                (r.left + r.right - width) / 2
            };
            let y = if position.starts_with("top") {
                r.top + margin
            } else if position.starts_with("center") {
                (r.top + r.bottom - height) / 2
            } else {
                r.bottom - height - margin
            };

            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, width, height, SWP_NOACTIVATE);
            let rgn = CreateRoundRectRgn(0, 0, width + 1, height + 1, 20, 20);
            SetWindowRgn(self.hwnd, Some(rgn), true);
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            let _ = InvalidateRect(Some(self.hwnd), None, true);
            SetTimer(Some(self.hwnd), TIMER_ID, millis.max(200), None);
        }
    }
}

unsafe fn make_font() -> HFONT {
    let face = wide("Segoe UI");
    unsafe {
        CreateFontW(
            -36,
            0,
            0,
            0,
            600,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            PCWSTR(face.as_ptr()),
        )
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let brush = CreateSolidBrush(COLORREF(0x0020_2020));
                FillRect(hdc, &rc, brush);
                let _ = DeleteObject(brush.into());
                let font = make_font();
                let old = SelectObject(hdc, font.into());
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, COLORREF(0x00FF_FFFF));
                let mut text = TEXT.with(|t| wide(&t.borrow()));
                let n = text.len() - 1;
                DrawTextW(hdc, &mut text[..n], &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                SelectObject(hdc, old);
                let _ = DeleteObject(font.into());
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_TIMER => {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
