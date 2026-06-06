#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
use arboard::Clipboard;

// ── macOS focus management via ObjC runtime (no extra deps, no permissions needed) ──

#[cfg(target_os = "macos")]
mod macos_focus {
    use std::ffi::c_void;

    extern "C" {
        fn objc_getClass(name: *const u8) -> *mut c_void;
        fn sel_registerName(name: *const u8) -> *mut c_void;
        fn objc_msgSend();
    }

    /// Get the PID of the currently frontmost application.
    pub fn get_frontmost_pid() -> i32 {
        unsafe {
            type SendObj = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
            type SendI32 = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i32;
            let send_obj: SendObj = std::mem::transmute(objc_msgSend as *const ());
            let send_i32: SendI32 = std::mem::transmute(objc_msgSend as *const ());

            let cls = objc_getClass(b"NSWorkspace\0".as_ptr());
            let workspace = send_obj(cls, sel_registerName(b"sharedWorkspace\0".as_ptr()));
            let app = send_obj(
                workspace,
                sel_registerName(b"frontmostApplication\0".as_ptr()),
            );
            if app.is_null() {
                return -1;
            }
            send_i32(app, sel_registerName(b"processIdentifier\0".as_ptr()))
        }
    }

    /// Bring an application to the foreground by its PID.
    pub fn activate_pid(pid: i32) {
        unsafe {
            type SendWithI32 = unsafe extern "C" fn(*mut c_void, *mut c_void, i32) -> *mut c_void;
            type SendWithUsize = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> bool;
            let send_with_i32: SendWithI32 = std::mem::transmute(objc_msgSend as *const ());
            let send_with_usize: SendWithUsize = std::mem::transmute(objc_msgSend as *const ());

            let cls = objc_getClass(b"NSRunningApplication\0".as_ptr());
            let app = send_with_i32(
                cls,
                sel_registerName(b"runningApplicationWithProcessIdentifier:\0".as_ptr()),
                pid,
            );
            if !app.is_null() {
                // NSApplicationActivateIgnoringOtherApps = 2
                send_with_usize(app, sel_registerName(b"activateWithOptions:\0".as_ptr()), 2);
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos_focus::{activate_pid, get_frontmost_pid};

// ── Windows focus management via Win32 (no extra deps) ──
#[cfg(target_os = "windows")]
mod win_focus {
    use std::ffi::c_void;

    type HWND = *mut c_void;
    type DWORD = u32;
    type BOOL = i32;

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> HWND;
        fn GetWindowThreadProcessId(hwnd: HWND, lpdwProcessId: *mut DWORD) -> DWORD;
        fn SetForegroundWindow(hwnd: HWND) -> BOOL;
        fn IsWindow(hwnd: HWND) -> BOOL;
        fn IsWindowVisible(hwnd: HWND) -> BOOL;
        fn IsIconic(hwnd: HWND) -> BOOL;
        fn EnumWindows(lpEnumFunc: extern "system" fn(HWND, isize) -> BOOL, lParam: isize) -> BOOL;
        fn AttachThreadInput(idAttach: DWORD, idAttachTo: DWORD, fAttach: BOOL) -> BOOL;
        fn BringWindowToTop(hwnd: HWND) -> BOOL;
        fn ShowWindow(hwnd: HWND, nCmdShow: i32) -> BOOL;
        fn GetWindowTextW(hwnd: HWND, lpString: *mut u16, nMaxCount: i32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> DWORD;
    }

    fn window_title(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        if n <= 0 {
            String::from("<no-title>")
        } else {
            String::from_utf16_lossy(&buf[..n as usize])
        }
    }

    fn pid_of(hwnd: HWND) -> u32 {
        let mut pid: DWORD = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid as *mut DWORD) };
        pid
    }

    /// PID of the currently-foreground window. `-1` if there is no foreground.
    pub fn get_frontmost_pid() -> i32 {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                eprintln!("[focus/win] GetForegroundWindow returned NULL");
                return -1;
            }
            let pid = pid_of(hwnd);
            let title = window_title(hwnd);
            eprintln!(
                "[focus/win] foreground hwnd={:p}, pid={}, title={:?}",
                hwnd, pid, title
            );
            pid as i32
        }
    }

    // EnumWindows callback: find a visible top-level window owned by the target PID.
    #[repr(C)]
    struct FindCtx {
        target_pid: u32,
        found: HWND,
    }

    extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> BOOL {
        unsafe {
            let ctx = &mut *(lparam as *mut FindCtx);
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }
            if pid_of(hwnd) == ctx.target_pid {
                ctx.found = hwnd;
                return 0; // stop
            }
            1 // continue
        }
    }

    /// Bring an application to the foreground by its PID, using the
    /// AttachThreadInput trick so SetForegroundWindow actually succeeds when
    /// our process isn't the foreground owner.
    pub fn activate_pid(pid: i32) {
        if pid <= 0 {
            eprintln!("[focus/win] activate_pid: skipping invalid pid={}", pid);
            return;
        }
        unsafe {
            let mut ctx = FindCtx {
                target_pid: pid as u32,
                found: std::ptr::null_mut(),
            };
            EnumWindows(enum_proc, &mut ctx as *mut FindCtx as isize);

            if ctx.found.is_null() {
                eprintln!(
                    "[focus/win] activate_pid({}): no visible top-level window found",
                    pid
                );
                return;
            }
            if IsWindow(ctx.found) == 0 {
                eprintln!("[focus/win] activate_pid({}): hwnd no longer valid", pid);
                return;
            }

            let target_thread = GetWindowThreadProcessId(ctx.found, std::ptr::null_mut());
            let our_thread = GetCurrentThreadId();
            let attached = AttachThreadInput(our_thread, target_thread, 1) != 0;

            // Only restore if minimised — calling SW_RESTORE on a fullscreen
            // window forces it out of fullscreen mode. SW_RESTORE = 9.
            let was_minimised = IsIconic(ctx.found) != 0;
            if was_minimised {
                ShowWindow(ctx.found, 9);
            }
            BringWindowToTop(ctx.found);
            let ok = SetForegroundWindow(ctx.found) != 0;

            if attached {
                AttachThreadInput(our_thread, target_thread, 0);
            }

            let title = window_title(ctx.found);
            eprintln!(
                "[focus/win] activate_pid({}) -> hwnd={:p}, title={:?}, minimised={}, SetForegroundWindow={}",
                pid, ctx.found, title, was_minimised, ok
            );
        }
    }
}

#[cfg(target_os = "windows")]
pub use win_focus::{activate_pid, get_frontmost_pid};

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub fn get_frontmost_pid() -> i32 {
    -1
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub fn activate_pid(_pid: i32) {}

/// Inject text into the currently focused control.
///
/// Both Windows and macOS take the "type Unicode characters directly" path:
/// - Windows: `SendInput` with `KEYEVENTF_UNICODE` (via `enigo.text`)
/// - macOS: `CGEventKeyboardSetUnicodeString` (via `CGEvent::set_string`)
///
/// Both bypass the clipboard and Cmd/Ctrl+V entirely. The clipboard route
/// is brittle in editors that remap paste, route key events to different
/// child windows (Electron), or compete with us on clipboard ownership.
///
/// Linux keeps clipboard + Ctrl+V because synthesised input on Wayland is
/// security-restricted and X11's Unicode key support is uneven.
#[cfg(target_os = "windows")]
pub fn paste_text(text: &str) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};

    let t0 = std::time::Instant::now();
    eprintln!(
        "[paste] (unicode-keys) enter: text_len={} chars",
        text.chars().count()
    );

    // Foreground sanity check right before injection so any failure log
    // shows whether focus drifted between restore_focus() and here.
    let pid_now = win_focus::get_frontmost_pid();
    eprintln!("[paste] foreground pid at injection time: {}", pid_now);

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| {
        eprintln!("[paste] Enigo::new failed: {}", e);
        format!("Enigo init error: {}", e)
    })?;

    enigo.text(text).map_err(|e| {
        eprintln!("[paste] enigo.text FAILED: {}", e);
        format!("Text injection error: {}", e)
    })?;

    eprintln!(
        "[paste] (unicode-keys) done in {}ms",
        t0.elapsed().as_millis()
    );
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn paste_text(text: &str) -> Result<(), String> {
    use core_graphics::event::{CGEvent, CGEventTapLocation};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let t0 = std::time::Instant::now();
    eprintln!(
        "[paste] (cg-unicode) enter: text_len={} chars",
        text.chars().count()
    );

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEventSource")?;

    // Build a single key-down event with no virtual keycode and stamp the
    // entire string onto it as Unicode. CG dispatches it as if an IME had
    // committed the text — much more reliable than Cmd+V via the clipboard.
    let key_down = CGEvent::new_keyboard_event(source.clone(), 0, true)
        .map_err(|_| "Failed to create key down event")?;
    key_down.set_string(text);
    key_down.post(CGEventTapLocation::HID);

    let key_up = CGEvent::new_keyboard_event(source, 0, false)
        .map_err(|_| "Failed to create key up event")?;
    key_up.set_string(text);
    key_up.post(CGEventTapLocation::HID);

    eprintln!(
        "[paste] (cg-unicode) done in {}ms",
        t0.elapsed().as_millis()
    );
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub fn paste_text(text: &str) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    eprintln!("[paste] enter: text_len={} chars", text.chars().count());

    // Save current clipboard content (best effort)
    let mut clipboard = Clipboard::new().map_err(|e| {
        eprintln!("[paste] Clipboard::new failed: {}", e);
        format!("Clipboard init error: {}", e)
    })?;

    let previous = match clipboard.get_text() {
        Ok(s) => {
            eprintln!(
                "[paste] saved previous clipboard ({} chars)",
                s.chars().count()
            );
            Some(s)
        }
        Err(e) => {
            eprintln!(
                "[paste] could not read previous clipboard: {} (continuing)",
                e
            );
            None
        }
    };

    // Set new text
    clipboard.set_text(text).map_err(|e| {
        eprintln!("[paste] clipboard.set_text FAILED: {}", e);
        format!("Clipboard set error: {}", e)
    })?;
    eprintln!(
        "[paste] clipboard.set_text ok ({}ms)",
        t0.elapsed().as_millis()
    );

    // Small delay to ensure clipboard is ready
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Simulate Cmd+V / Ctrl+V
    let t_sim = std::time::Instant::now();
    if let Err(e) = simulate_paste() {
        eprintln!(
            "[paste] simulate_paste FAILED after {}ms: {}",
            t_sim.elapsed().as_millis(),
            e
        );
        // Still try to restore clipboard before bailing
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(prev) = previous {
            let _ = clipboard.set_text(&prev);
        }
        return Err(e);
    }
    eprintln!(
        "[paste] simulate_paste ok ({}ms)",
        t_sim.elapsed().as_millis()
    );

    // Small delay then restore clipboard (best effort)
    std::thread::sleep(std::time::Duration::from_millis(100));
    if let Some(prev) = previous {
        match clipboard.set_text(&prev) {
            Ok(()) => eprintln!("[paste] restored previous clipboard"),
            Err(e) => eprintln!("[paste] failed to restore clipboard: {}", e),
        }
    }

    eprintln!("[paste] done in {}ms", t0.elapsed().as_millis());
    Ok(())
}

/// Check (and optionally prompt for) macOS Accessibility permission.
/// Returns true if the app is already trusted.
#[cfg(target_os = "macos")]
pub fn ensure_accessibility_permission() -> bool {
    use std::ffi::c_void;

    #[repr(C)]
    struct CFDictionaryKeyCallBacks {
        _opaque: [u8; 0],
    }
    #[repr(C)]
    struct CFDictionaryValueCallBacks {
        _opaque: [u8; 0],
    }

    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;

        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num_values: isize,
            key_callbacks: *const CFDictionaryKeyCallBacks,
            value_callbacks: *const CFDictionaryValueCallBacks,
        ) -> *const c_void;
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            c_str: *const i8,
            encoding: u32,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);

        static kCFBooleanTrue: *const c_void;
        static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
        static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

    unsafe {
        let key = CFStringCreateWithCString(
            std::ptr::null(),
            b"AXTrustedCheckOptionPrompt\0".as_ptr() as *const i8,
            K_CF_STRING_ENCODING_UTF8,
        );

        let keys = [key];
        let values = [kCFBooleanTrue];

        let dict = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr() as *const *const c_void,
            values.as_ptr() as *const *const c_void,
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );

        let trusted = AXIsProcessTrustedWithOptions(dict);

        CFRelease(dict);
        CFRelease(key);

        trusted
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_accessibility_permission() -> bool {
    true
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn simulate_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| {
        eprintln!("[paste/sim] Enigo::new failed: {}", e);
        format!("Enigo init error: {}", e)
    })?;

    eprintln!("[paste/sim] Ctrl down");
    enigo.key(Key::Control, Direction::Press).map_err(|e| {
        eprintln!("[paste/sim] Ctrl press error: {}", e);
        format!("Key press error: {}", e)
    })?;
    eprintln!("[paste/sim] V click");
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| {
            eprintln!("[paste/sim] V click error: {}", e);
            format!("Key click error: {}", e)
        })?;
    eprintln!("[paste/sim] Ctrl up");
    enigo.key(Key::Control, Direction::Release).map_err(|e| {
        eprintln!("[paste/sim] Ctrl release error: {}", e);
        format!("Key release error: {}", e)
    })?;

    Ok(())
}
