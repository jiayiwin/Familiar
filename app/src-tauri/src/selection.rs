//! The text selected in the window in front, read through UI Automation: no
//! clipboard, no keystrokes. For 「记一件事」 on its hotkey. A password box is
//! never read.

/// What is selected where the keyboard focus is, if anything (one line, at most 400 characters read).
/// Blocks for up to a couple of seconds on an unresponsive window: call it off the main thread.
#[cfg(windows)]
pub fn selected() -> Option<String> {
    use ::windows::core::Interface;
    use ::windows::Win32::System::Com::*;
    use ::windows::Win32::UI::Accessibility::*;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let a: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        if let Ok(a2) = a.cast::<IUIAutomation2>() {
            let _ = a2.SetConnectionTimeout(1500);
            let _ = a2.SetTransactionTimeout(1500);
        }
        let el = a.GetFocusedElement().ok()?;
        if el.CurrentIsPassword().is_ok_and(|p| p.as_bool()) {
            return None;
        }
        let tp: IUIAutomationTextPattern = el.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let ranges = tp.GetSelection().ok()?;
        let mut out = String::new();
        for i in 0..ranges.Length().ok()? {
            if let Ok(t) = ranges.GetElement(i).and_then(|r| r.GetText(400)) {
                out.push_str(&t.to_string());
                out.push(' ');
            }
        }
        let s = out.split_whitespace().collect::<Vec<_>>().join(" ");
        (!s.is_empty()).then_some(s)
    }
}

#[cfg(not(windows))]
pub fn selected() -> Option<String> {
    None
}
