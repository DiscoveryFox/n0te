//! The Windows touch keyboard for n0te Tablet. Windows shows it by itself only for text fields
//! it can see through UI Automation, which iced doesn't provide yet; so when typing starts on a
//! device without a hardware keyboard (text tool, search, invites), n0te Tablet opens it: the
//! keyboard host's `ITipInvocation::Toggle` (the interface Windows' own shell uses), starting
//! TabTip.exe first if it isn't running. Elsewhere this does nothing.

/// Shows the touch keyboard (Windows; true if it was asked to show).
pub fn show() -> bool {
    #[cfg(windows)]
    {
        win::toggle_visible(true)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Hides it again (when typing ends).
pub fn hide() -> bool {
    #[cfg(windows)]
    {
        win::toggle_visible(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::Com::{CLSCTX_INPROC_HANDLER, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx};
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetDesktopWindow, GetWindowLongW, GWL_STYLE, WS_DISABLED};
    use windows_sys::core::GUID;

    /// CLSID_UIHostNoLaunch: the touch keyboard host.
    const CLSID_UIHOST_NO_LAUNCH: GUID = GUID::from_u128(0x4ce576fa_83dc_4f88_951c_9d0782b4e376);
    /// IID_ITipInvocation.
    const IID_ITIP_INVOCATION: GUID = GUID::from_u128(0x37c994e7_432b_4834_a2f7_dce1f13b834b);

    #[repr(C)]
    struct TipVtbl {
        query_interface: unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        toggle: unsafe extern "system" fn(*mut c_void, HWND) -> i32,
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Whether the keyboard window is up (its frame exists and isn't disabled).
    fn visible() -> bool {
        let class = wide("IPTip_Main_Window");
        // SAFETY: plain window lookups.
        unsafe {
            let w = FindWindowW(class.as_ptr(), std::ptr::null());
            !w.is_null() && (GetWindowLongW(w, GWL_STYLE) as u32 & WS_DISABLED) == 0
        }
    }

    fn start_tabtip() {
        let base = std::env::var("CommonProgramW6432").or_else(|_| std::env::var("CommonProgramFiles")).unwrap_or_else(|_| r"C:\Program Files\Common Files".into());
        let exe = std::path::Path::new(&base).join(r"microsoft shared\ink\TabTip.exe");
        if exe.exists() {
            let _ = n0te_platform::spawn_detached(&exe, &[]);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }

    pub fn toggle_visible(want: bool) -> bool {
        if visible() == want {
            return want;
        }
        if want {
            start_tabtip();
        }
        // SAFETY: COM on this thread, then one call through the interface's vtable.
        unsafe {
            CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            let mut tip: *mut c_void = std::ptr::null_mut();
            let hr = CoCreateInstance(&CLSID_UIHOST_NO_LAUNCH, std::ptr::null_mut(), CLSCTX_INPROC_HANDLER | CLSCTX_LOCAL_SERVER, &IID_ITIP_INVOCATION, &mut tip);
            if hr < 0 || tip.is_null() {
                return false;
            }
            let vtbl = *(tip as *const *const TipVtbl);
            let ok = ((*vtbl).toggle)(tip, GetDesktopWindow()) >= 0;
            ((*vtbl).release)(tip);
            ok
        }
    }
}
