#![allow(non_upper_case_globals)]
#![allow(dead_code)]

use crate::engine::{
    physical_down_set, InputCmd, InputInjector, KeySpec, MouseButton, Pulse, SendReport,
    VIRTUAL_KEY, ENGINE, EXTRA_INFO,
};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::CFString;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventType, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::collections::HashSet;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// =========================================================================
// macOS Virtual Keycodes (from Carbon HIToolbox / Events.h)
// =========================================================================
pub const kVK_ANSI_A: u16 = 0x00;
pub const kVK_ANSI_S: u16 = 0x01;
pub const kVK_ANSI_D: u16 = 0x02;
pub const kVK_ANSI_F: u16 = 0x03;
pub const kVK_ANSI_H: u16 = 0x04;
pub const kVK_ANSI_G: u16 = 0x05;
pub const kVK_ANSI_Z: u16 = 0x06;
pub const kVK_ANSI_X: u16 = 0x07;
pub const kVK_ANSI_C: u16 = 0x08;
pub const kVK_ANSI_V: u16 = 0x09;
pub const kVK_ANSI_B: u16 = 0x0B;
pub const kVK_ANSI_Q: u16 = 0x0C;
pub const kVK_ANSI_W: u16 = 0x0D;
pub const kVK_ANSI_E: u16 = 0x0E;
pub const kVK_ANSI_R: u16 = 0x0F;
pub const kVK_ANSI_Y: u16 = 0x10;
pub const kVK_ANSI_T: u16 = 0x11;
pub const kVK_ANSI_1: u16 = 0x12;
pub const kVK_ANSI_2: u16 = 0x13;
pub const kVK_ANSI_3: u16 = 0x14;
pub const kVK_ANSI_4: u16 = 0x15;
pub const kVK_ANSI_6: u16 = 0x16;
pub const kVK_ANSI_5: u16 = 0x17;
pub const kVK_ANSI_Equal: u16 = 0x18;
pub const kVK_ANSI_9: u16 = 0x19;
pub const kVK_ANSI_7: u16 = 0x1A;
pub const kVK_ANSI_Minus: u16 = 0x1B;
pub const kVK_ANSI_8: u16 = 0x1C;
pub const kVK_ANSI_0: u16 = 0x1D;
pub const kVK_ANSI_RightBracket: u16 = 0x1E;
pub const kVK_ANSI_O: u16 = 0x1F;
pub const kVK_ANSI_U: u16 = 0x20;
pub const kVK_ANSI_LeftBracket: u16 = 0x21;
pub const kVK_ANSI_I: u16 = 0x22;
pub const kVK_ANSI_P: u16 = 0x23;
pub const kVK_ANSI_L: u16 = 0x25;
pub const kVK_ANSI_J: u16 = 0x26;
pub const kVK_ANSI_Quote: u16 = 0x27;
pub const kVK_ANSI_K: u16 = 0x28;
pub const kVK_ANSI_Semicolon: u16 = 0x29;
pub const kVK_ANSI_Backslash: u16 = 0x2A;
pub const kVK_ANSI_Comma: u16 = 0x2B;
pub const kVK_ANSI_Slash: u16 = 0x2C;
pub const kVK_ANSI_N: u16 = 0x2D;
pub const kVK_ANSI_M: u16 = 0x2E;
pub const kVK_ANSI_Period: u16 = 0x2F;
pub const kVK_ANSI_Grave: u16 = 0x32; // ` Backquote
pub const kVK_ANSI_KeypadDecimal: u16 = 0x41;
pub const kVK_ANSI_KeypadMultiply: u16 = 0x43;
pub const kVK_ANSI_KeypadPlus: u16 = 0x45;
pub const kVK_ANSI_KeypadClear: u16 = 0x47;
pub const kVK_ANSI_KeypadDivide: u16 = 0x4B;
pub const kVK_ANSI_KeypadEnter: u16 = 0x4C;
pub const kVK_ANSI_KeypadMinus: u16 = 0x4E;
pub const kVK_ANSI_KeypadEquals: u16 = 0x51;
pub const kVK_ANSI_Keypad0: u16 = 0x52;
pub const kVK_ANSI_Keypad1: u16 = 0x53;
pub const kVK_ANSI_Keypad2: u16 = 0x54;
pub const kVK_ANSI_Keypad3: u16 = 0x55;
pub const kVK_ANSI_Keypad4: u16 = 0x56;
pub const kVK_ANSI_Keypad5: u16 = 0x57;
pub const kVK_ANSI_Keypad6: u16 = 0x58;
pub const kVK_ANSI_Keypad7: u16 = 0x59;
pub const kVK_ANSI_Keypad8: u16 = 0x5B;
pub const kVK_ANSI_Keypad9: u16 = 0x5C;

pub const kVK_Return: u16 = 0x24; // 36
pub const kVK_Tab: u16 = 0x30; // 48
pub const kVK_Space: u16 = 0x31; // 49
pub const kVK_Delete: u16 = 0x33; // 51 (Backspace)
pub const kVK_Escape: u16 = 0x35; // 53
pub const kVK_Command: u16 = 0x37; // 55
pub const kVK_Shift: u16 = 0x38; // 56
pub const kVK_CapsLock: u16 = 0x39; // 57
pub const kVK_Option: u16 = 0x3A; // 58
pub const kVK_Control: u16 = 0x3B; // 59
pub const kVK_RightShift: u16 = 0x3C; // 60
pub const kVK_RightOption: u16 = 0x3D; // 61
pub const kVK_RightControl: u16 = 0x3E; // 62
pub const kVK_RightCommand: u16 = 0x36; // 54
pub const kVK_ForwardDelete: u16 = 0x75; // 117
pub const kVK_Home: u16 = 0x73; // 115
pub const kVK_End: u16 = 0x77; // 119
pub const kVK_PageUp: u16 = 0x74; // 116
pub const kVK_PageDown: u16 = 0x79; // 121
pub const kVK_LeftArrow: u16 = 0x7B; // 123
pub const kVK_RightArrow: u16 = 0x7C; // 124
pub const kVK_DownArrow: u16 = 0x7D; // 125
pub const kVK_UpArrow: u16 = 0x7E; // 126

pub const kVK_F1: u16 = 0x7A; // 122
pub const kVK_F2: u16 = 0x78; // 120
pub const kVK_F3: u16 = 0x63; // 99
pub const kVK_F4: u16 = 0x76; // 118
pub const kVK_F5: u16 = 0x60; // 96
pub const kVK_F6: u16 = 0x61; // 97
pub const kVK_F7: u16 = 0x62; // 98
pub const kVK_F8: u16 = 0x64; // 100
pub const kVK_F9: u16 = 0x65; // 101
pub const kVK_F10: u16 = 0x6D; // 109
pub const kVK_F11: u16 = 0x67; // 103
pub const kVK_F12: u16 = 0x6F; // 111
pub const kVK_F13: u16 = 0x69; // 105
pub const kVK_F14: u16 = 0x6B; // 107
pub const kVK_F15: u16 = 0x71; // 113
pub const kVK_F16: u16 = 0x6A; // 106
pub const kVK_F17: u16 = 0x40; // 64
pub const kVK_F18: u16 = 0x4F; // 79
pub const kVK_F19: u16 = 0x50; // 80
pub const kVK_F20: u16 = 0x5A; // 90
pub const kVK_Help: u16 = 0x72; // 114 (Insert on PC keyboards)

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
}

pub fn check_accessibility_permission() -> bool {
    let key = CFString::new("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::true_value();
    let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef()) }
}

pub fn is_modifier_keycode(code: u16) -> bool {
    matches!(
        code,
        kVK_Command
            | kVK_RightCommand
            | kVK_Option
            | kVK_RightOption
            | kVK_Control
            | kVK_RightControl
            | kVK_Shift
            | kVK_RightShift
            | kVK_CapsLock
    )
}

// =========================================================================
// Token <-> macOS Keycode mapping
// =========================================================================
pub fn key_spec(name: &str) -> Option<KeySpec> {
    let n = name.trim();
    let code = match n {
        "LControl" | "Control" | "Ctrl" => kVK_Control,
        "RControl" => kVK_RightControl,
        "LAlt" | "Alt" | "Option" | "LOption" => kVK_Option,
        "RAlt" | "ROption" => kVK_RightOption,
        "LShift" | "Shift" => kVK_Shift,
        "RShift" => kVK_RightShift,
        "LWin" | "Meta" | "Win" | "Command" | "Cmd" | "LCommand" => kVK_Command,
        "RWin" | "RCommand" => kVK_RightCommand,
        "Space" => kVK_Space,
        "Enter" | "Return" => kVK_Return,
        "Tab" => kVK_Tab,
        "Escape" | "Esc" => kVK_Escape,
        "Backspace" => kVK_Delete,
        "Delete" => kVK_ForwardDelete,
        "Insert" => kVK_Help,
        "Home" => kVK_Home,
        "End" => kVK_End,
        "PageUp" => kVK_PageUp,
        "PageDown" => kVK_PageDown,
        "ArrowLeft" => kVK_LeftArrow,
        "ArrowRight" => kVK_RightArrow,
        "ArrowUp" => kVK_UpArrow,
        "ArrowDown" => kVK_DownArrow,
        "Minus" | "-" => kVK_ANSI_Minus,
        "Equal" | "=" => kVK_ANSI_Equal,
        "Comma" | "," => kVK_ANSI_Comma,
        "Period" | "." => kVK_ANSI_Period,
        "Slash" | "/" => kVK_ANSI_Slash,
        "Backquote" | "`" => kVK_ANSI_Grave,
        "BracketLeft" | "[" => kVK_ANSI_LeftBracket,
        "Backslash" | "\\" => kVK_ANSI_Backslash,
        "BracketRight" | "]" => kVK_ANSI_RightBracket,
        "Quote" | "'" => kVK_ANSI_Quote,
        "Semicolon" | ";" => kVK_ANSI_Semicolon,
        "F1" => kVK_F1,
        "F2" => kVK_F2,
        "F3" => kVK_F3,
        "F4" => kVK_F4,
        "F5" => kVK_F5,
        "F6" => kVK_F6,
        "F7" => kVK_F7,
        "F8" => kVK_F8,
        "F9" => kVK_F9,
        "F10" => kVK_F10,
        "F11" => kVK_F11,
        "F12" => kVK_F12,
        "F13" => kVK_F13,
        "F14" => kVK_F14,
        "F15" => kVK_F15,
        "F16" => kVK_F16,
        "F17" => kVK_F17,
        "F18" => kVK_F18,
        "F19" => kVK_F19,
        "F20" => kVK_F20,
        "A" | "a" => kVK_ANSI_A,
        "B" | "b" => kVK_ANSI_B,
        "C" | "c" => kVK_ANSI_C,
        "D" | "d" => kVK_ANSI_D,
        "E" | "e" => kVK_ANSI_E,
        "F" | "f" => kVK_ANSI_F,
        "G" | "g" => kVK_ANSI_G,
        "H" | "h" => kVK_ANSI_H,
        "I" | "i" => kVK_ANSI_I,
        "J" | "j" => kVK_ANSI_J,
        "K" | "k" => kVK_ANSI_K,
        "L" | "l" => kVK_ANSI_L,
        "M" | "m" => kVK_ANSI_M,
        "N" | "n" => kVK_ANSI_N,
        "O" | "o" => kVK_ANSI_O,
        "P" | "p" => kVK_ANSI_P,
        "Q" | "q" => kVK_ANSI_Q,
        "R" | "r" => kVK_ANSI_R,
        "S" | "s" => kVK_ANSI_S,
        "T" | "t" => kVK_ANSI_T,
        "U" | "u" => kVK_ANSI_U,
        "V" | "v" => kVK_ANSI_V,
        "W" | "w" => kVK_ANSI_W,
        "X" | "x" => kVK_ANSI_X,
        "Y" | "y" => kVK_ANSI_Y,
        "Z" | "z" => kVK_ANSI_Z,
        "0" => kVK_ANSI_0,
        "1" => kVK_ANSI_1,
        "2" => kVK_ANSI_2,
        "3" => kVK_ANSI_3,
        "4" => kVK_ANSI_4,
        "5" => kVK_ANSI_5,
        "6" => kVK_ANSI_6,
        "7" => kVK_ANSI_7,
        "8" => kVK_ANSI_8,
        "9" => kVK_ANSI_9,
        "Numpad0" => kVK_ANSI_Keypad0,
        "Numpad1" => kVK_ANSI_Keypad1,
        "Numpad2" => kVK_ANSI_Keypad2,
        "Numpad3" => kVK_ANSI_Keypad3,
        "Numpad4" => kVK_ANSI_Keypad4,
        "Numpad5" => kVK_ANSI_Keypad5,
        "Numpad6" => kVK_ANSI_Keypad6,
        "Numpad7" => kVK_ANSI_Keypad7,
        "Numpad8" => kVK_ANSI_Keypad8,
        "Numpad9" => kVK_ANSI_Keypad9,
        "NumpadMultiply" => kVK_ANSI_KeypadMultiply,
        "NumpadAdd" => kVK_ANSI_KeypadPlus,
        "NumpadSubtract" => kVK_ANSI_KeypadMinus,
        "NumpadDecimal" => kVK_ANSI_KeypadDecimal,
        "NumpadDivide" => kVK_ANSI_KeypadDivide,
        "NumpadEnter" => kVK_ANSI_KeypadEnter,
        _ => {
            let digits = match n.strip_prefix('F') {
                Some(d) if d.len() <= 2 => d,
                _ => return None,
            };
            match digits.parse::<u16>() {
                Ok(1) => kVK_F1,
                Ok(2) => kVK_F2,
                Ok(3) => kVK_F3,
                Ok(4) => kVK_F4,
                Ok(5) => kVK_F5,
                Ok(6) => kVK_F6,
                Ok(7) => kVK_F7,
                Ok(8) => kVK_F8,
                Ok(9) => kVK_F9,
                Ok(10) => kVK_F10,
                Ok(11) => kVK_F11,
                Ok(12) => kVK_F12,
                Ok(13) => kVK_F13,
                Ok(14) => kVK_F14,
                Ok(15) => kVK_F15,
                Ok(16) => kVK_F16,
                Ok(17) => kVK_F17,
                Ok(18) => kVK_F18,
                Ok(19) => kVK_F19,
                Ok(20) => kVK_F20,
                _ => return None,
            }
        }
    };
    Some(KeySpec {
        vk: VIRTUAL_KEY(code),
        extended: false,
    })
}

pub fn keycode_to_token(code: u16) -> Option<String> {
    Some(match code {
        kVK_Command => "LWin".into(),
        kVK_RightCommand => "RWin".into(),
        kVK_Option => "LAlt".into(),
        kVK_RightOption => "RAlt".into(),
        kVK_Control => "LControl".into(),
        kVK_RightControl => "RControl".into(),
        kVK_Shift => "LShift".into(),
        kVK_RightShift => "RShift".into(),
        kVK_Space => "Space".into(),
        kVK_Return => "Enter".into(),
        kVK_Tab => "Tab".into(),
        kVK_Escape => "Escape".into(),
        kVK_Delete => "Backspace".into(),
        kVK_ForwardDelete => "Delete".into(),
        kVK_Home => "Home".into(),
        kVK_End => "End".into(),
        kVK_PageUp => "PageUp".into(),
        kVK_PageDown => "PageDown".into(),
        kVK_LeftArrow => "ArrowLeft".into(),
        kVK_RightArrow => "ArrowRight".into(),
        kVK_UpArrow => "ArrowUp".into(),
        kVK_DownArrow => "ArrowDown".into(),
        kVK_ANSI_Minus => "Minus".into(),
        kVK_ANSI_Equal => "Equal".into(),
        kVK_ANSI_Comma => "Comma".into(),
        kVK_ANSI_Period => "Period".into(),
        kVK_ANSI_Slash => "Slash".into(),
        kVK_ANSI_Grave => "Backquote".into(),
        kVK_ANSI_LeftBracket => "BracketLeft".into(),
        kVK_ANSI_Backslash => "Backslash".into(),
        kVK_ANSI_RightBracket => "BracketRight".into(),
        kVK_ANSI_Quote => "Quote".into(),
        kVK_ANSI_Semicolon => "Semicolon".into(),
        kVK_F1 => "F1".into(),
        kVK_F2 => "F2".into(),
        kVK_F3 => "F3".into(),
        kVK_F4 => "F4".into(),
        kVK_F5 => "F5".into(),
        kVK_F6 => "F6".into(),
        kVK_F7 => "F7".into(),
        kVK_F8 => "F8".into(),
        kVK_F9 => "F9".into(),
        kVK_F10 => "F10".into(),
        kVK_F11 => "F11".into(),
        kVK_F12 => "F12".into(),
        kVK_F13 => "F13".into(),
        kVK_F14 => "F14".into(),
        kVK_F15 => "F15".into(),
        kVK_F16 => "F16".into(),
        kVK_F17 => "F17".into(),
        kVK_F18 => "F18".into(),
        kVK_F19 => "F19".into(),
        kVK_F20 => "F20".into(),
        kVK_Help => "Insert".into(),
        kVK_ANSI_A => "A".into(),
        kVK_ANSI_B => "B".into(),
        kVK_ANSI_C => "C".into(),
        kVK_ANSI_D => "D".into(),
        kVK_ANSI_E => "E".into(),
        kVK_ANSI_F => "F".into(),
        kVK_ANSI_G => "G".into(),
        kVK_ANSI_H => "H".into(),
        kVK_ANSI_I => "I".into(),
        kVK_ANSI_J => "J".into(),
        kVK_ANSI_K => "K".into(),
        kVK_ANSI_L => "L".into(),
        kVK_ANSI_M => "M".into(),
        kVK_ANSI_N => "N".into(),
        kVK_ANSI_O => "O".into(),
        kVK_ANSI_P => "P".into(),
        kVK_ANSI_Q => "Q".into(),
        kVK_ANSI_R => "R".into(),
        kVK_ANSI_S => "S".into(),
        kVK_ANSI_T => "T".into(),
        kVK_ANSI_U => "U".into(),
        kVK_ANSI_V => "V".into(),
        kVK_ANSI_W => "W".into(),
        kVK_ANSI_X => "X".into(),
        kVK_ANSI_Y => "Y".into(),
        kVK_ANSI_Z => "Z".into(),
        kVK_ANSI_0 => "0".into(),
        kVK_ANSI_1 => "1".into(),
        kVK_ANSI_2 => "2".into(),
        kVK_ANSI_3 => "3".into(),
        kVK_ANSI_4 => "4".into(),
        kVK_ANSI_5 => "5".into(),
        kVK_ANSI_6 => "6".into(),
        kVK_ANSI_7 => "7".into(),
        kVK_ANSI_8 => "8".into(),
        kVK_ANSI_9 => "9".into(),
        kVK_ANSI_Keypad0 => "Numpad0".into(),
        kVK_ANSI_Keypad1 => "Numpad1".into(),
        kVK_ANSI_Keypad2 => "Numpad2".into(),
        kVK_ANSI_Keypad3 => "Numpad3".into(),
        kVK_ANSI_Keypad4 => "Numpad4".into(),
        kVK_ANSI_Keypad5 => "Numpad5".into(),
        kVK_ANSI_Keypad6 => "Numpad6".into(),
        kVK_ANSI_Keypad7 => "Numpad7".into(),
        kVK_ANSI_Keypad8 => "Numpad8".into(),
        kVK_ANSI_Keypad9 => "Numpad9".into(),
        kVK_ANSI_KeypadMultiply => "NumpadMultiply".into(),
        kVK_ANSI_KeypadPlus => "NumpadAdd".into(),
        kVK_ANSI_KeypadMinus => "NumpadSubtract".into(),
        kVK_ANSI_KeypadDecimal => "NumpadDecimal".into(),
        kVK_ANSI_KeypadDivide => "NumpadDivide".into(),
        kVK_ANSI_KeypadEnter => "Enter".into(),
        _ => return None,
    })
}

// =========================================================================
// MacosInjector: injects keyboard events via CoreGraphics
// =========================================================================
// NX_DEVICE* masks from IOLLEvent.h. Right Control is bit 13, not bit 7
// (bit 7 is NX_NONCOALSESCEDMASK).
fn modifier_flag(key: u16) -> CGEventFlags {
    let (generic, device) = match key {
        kVK_Command => (CGEventFlags::CGEventFlagCommand, 0x08),
        kVK_RightCommand => (CGEventFlags::CGEventFlagCommand, 0x10),
        kVK_Option => (CGEventFlags::CGEventFlagAlternate, 0x20),
        kVK_RightOption => (CGEventFlags::CGEventFlagAlternate, 0x40),
        kVK_Control => (CGEventFlags::CGEventFlagControl, 0x01),
        kVK_RightControl => (CGEventFlags::CGEventFlagControl, 0x2000),
        kVK_Shift => (CGEventFlags::CGEventFlagShift, 0x02),
        kVK_RightShift => (CGEventFlags::CGEventFlagShift, 0x04),
        kVK_CapsLock => (CGEventFlags::CGEventFlagAlphaShift, 0),
        _ => return CGEventFlags::empty(),
    };
    generic | CGEventFlags::from_bits_retain(device)
}

extern "C" {
    fn CGEventSourceFlagsState(state: CGEventSourceStateID) -> u64;
}

fn injected_flags(physical: CGEventFlags, held: &HashSet<u16>) -> CGEventFlags {
    held.iter().fold(physical, |flags, key| flags | modifier_flag(*key))
}

pub struct MacosInjector {
    active_modifiers: HashSet<u16>,
    source: CGEventSource,
}

unsafe impl Send for MacosInjector {}

impl MacosInjector {
    pub fn new() -> Option<Self> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok()?;
        Some(Self {
            active_modifiers: HashSet::new(),
            source,
        })
    }
}

/// Release keys the worker still believes are held, without touching the
/// worker thread. Used by the failsafe emergency path.
pub fn post_key_ups(specs: &[KeySpec]) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        log::error!("failsafe release: failed to create CGEventSource");
        return;
    };
    for spec in specs {
        if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), spec.vk.0, false) {
            if is_modifier_keycode(spec.vk.0) {
                event.set_type(CGEventType::FlagsChanged);
            }
            event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, EXTRA_INFO as i64);
            event.post(CGEventTapLocation::HID);
        }
    }
}

impl InputInjector for MacosInjector {
    fn send_keys(&mut self, specs: &[KeySpec], down: bool) -> Result<(), SendReport> {
        if specs.is_empty() {
            return Ok(());
        }

        for (inserted, spec) in specs.iter().enumerate() {
            let event = CGEvent::new_keyboard_event(self.source.clone(), spec.vk.0, down)
                .map_err(|_| SendReport {
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                    expected: specs.len() as u32, inserted: inserted as u32,
                    win32_error: 0, is_uipi_blocked: false,
                })?;
            // Only modifier keycodes carry flags; tracking anything else would
            // fold a bogus flag into every subsequent event.
            if is_modifier_keycode(spec.vk.0) {
                if down { self.active_modifiers.insert(spec.vk.0); }
                else { self.active_modifiers.remove(&spec.vk.0); }
            }
            // Carry the state at this event, not the final state of the batch.
            // A sibling modifier (left/right) and physical modifiers remain set.
            // HID state excludes our synthetic keys and includes modifiers already
            // held before startup, plus the actual Caps Lock state.
            let physical = CGEventFlags::from_bits_retain(unsafe {
                CGEventSourceFlagsState(CGEventSourceStateID::HIDSystemState)
            });
            event.set_flags(injected_flags(physical, &self.active_modifiers));
            if is_modifier_keycode(spec.vk.0) {
                event.set_type(CGEventType::FlagsChanged);
            }
            event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, EXTRA_INFO as i64);
            event.post(CGEventTapLocation::HID);
            // Ledger for the failsafe release path.
            let mut held = crate::engine::injected_held().lock();
            if down {
                held.insert(*spec);
            } else {
                held.remove(spec);
            }
        }
        Ok(())
    }

    fn relinquish_key(&mut self, spec: &KeySpec) {
        self.active_modifiers.remove(&spec.vk.0);
    }

    fn send_mask(&mut self) -> Result<(), SendReport> {
        // On macOS, modifier flags are explicitly carried on each CGEvent
        Ok(())
    }

    fn is_physical_down(&self, vk: VIRTUAL_KEY) -> bool {
        // Include keys held before our event tap was installed.
        unsafe { CGEventSourceKeyState(CGEventSourceStateID::HIDSystemState, vk.0) }
    }

    fn is_button_physically_down(&self, btn: MouseButton) -> bool {
        let num = match btn {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::XButton1 => 3,
            MouseButton::XButton2 => 4,
            _ => return false,
        };
        unsafe { CGEventSourceButtonState(CGEventSourceStateID::HIDSystemState, num) }
    }
}

// =========================================================================
// CoreGraphics Event Tap Global Listener
// =========================================================================
static TAP_PORT: AtomicUsize = AtomicUsize::new(0);
extern "C" {
    fn CGEventTapEnable(tap: *const std::ffi::c_void, enable: bool);
    fn CGEventSourceKeyState(state: CGEventSourceStateID, key: u16) -> bool;
    fn CGEventSourceButtonState(state: CGEventSourceStateID, button: u32) -> bool;
}

extern "C" {
    // core-foundation 0.10 exposes CFRunLoopStop but not CFRunLoopWakeUp.
    fn CFRunLoopWakeUp(rl: core_foundation::runloop::CFRunLoopRef);
}

static RUN_LOOP_REF: parking_lot::Mutex<Option<CFRunLoop>> = parking_lot::Mutex::new(None);
// Set when stop_hook() runs before the loop has published its runloop.
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn stop_hook() {
    let rl = RUN_LOOP_REF.lock().take();
    if let Some(rl) = rl {
        rl.stop();
        unsafe { CFRunLoopWakeUp(rl.as_concrete_TypeRef()) };
    } else {
        STOP_REQUESTED.store(true, Ordering::SeqCst);
    }
}

/// Respawn the event tap after a recoverable failure (e.g. Accessibility
/// permission was just granted). No-op while a loop is already live.
pub fn retry_hook() {
    if crate::engine::hook_status() == "ready" || RUN_LOOP_REF.lock().is_some() {
        return;
    }
    STOP_REQUESTED.store(false, Ordering::SeqCst);
    std::thread::spawn(hook_loop);
}

pub fn hook_loop() {
    if !check_accessibility_permission() {
        crate::engine::set_hook_status("macOS 尚未授予辅助功能权限。请在系统设置 → 隐私与安全性 → 辅助功能中添加当前安装的 Mouse Insight。若旧版本已勾选，请移除旧条目并重新添加，再启动应用。授权后点击「重试监听」即可，无需重启。");
        return;
    }

    let events_of_interest = vec![
        CGEventType::OtherMouseDown,
        CGEventType::OtherMouseUp,
        CGEventType::LeftMouseDown,
        CGEventType::LeftMouseUp,
        CGEventType::RightMouseDown,
        CGEventType::RightMouseUp,
        CGEventType::ScrollWheel,
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
    ];

    let tap = match CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        events_of_interest,
        |_proxy, etype, event| {
            handle_cgevent(etype, event)
        },
    ) {
        Ok(t) => t,
        Err(_) => {
            log::error!("Failed to create CGEventTap. Please ensure Accessibility permissions are granted.");
            if ENGINE.get().is_some() {
                crate::engine::set_hook_status("macOS 辅助功能已授权，但监听创建失败。请检查输入监控权限及其他鼠标工具；若刚替换应用，请重新添加当前应用的权限条目。");
            }
            return;
        }
    };

    let loop_source = match tap.mach_port().create_runloop_source(0) {
        Ok(s) => s,
        Err(_) => {
            log::error!("Failed to create runloop source for CGEventTap.");
            crate::engine::set_hook_status("鼠标监听启动失败，请重启应用。");
            return;
        }
    };

    let current_rl = CFRunLoop::get_current();
    current_rl.add_source(&loop_source, unsafe { kCFRunLoopCommonModes });
    TAP_PORT.store(tap.mach_port().as_concrete_TypeRef() as usize, Ordering::SeqCst);
    tap.enable();
    crate::engine::set_hook_status("ready");

    *RUN_LOOP_REF.lock() = Some(current_rl);
    if STOP_REQUESTED.swap(false, Ordering::SeqCst) {
        // stop_hook() ran before the runloop was published.
        *RUN_LOOP_REF.lock() = None;
        TAP_PORT.store(0, Ordering::SeqCst);
        return;
    }
    CFRunLoop::run_current();
    *RUN_LOOP_REF.lock() = None;
    TAP_PORT.store(0, Ordering::SeqCst);
}

fn handle_cgevent(etype: CGEventType, event: &CGEvent) -> CallbackResult {
    if etype == CGEventType::TapDisabledByUserInput {
        // The system or the user revoked the tap; re-enabling it would just be
        // disabled again. Surface it and let the user re-authorize + retry.
        crate::engine::set_hook_status("macOS 已禁用事件监听（权限被撤销或系统策略），请重新授权后点击「重试监听」");
        crate::engine::request_emergency_stop(false);
        return CallbackResult::Keep;
    }
    if etype == CGEventType::TapDisabledByTimeout {
        crate::engine::request_emergency_stop(false);
        let port = TAP_PORT.load(Ordering::SeqCst);
        if port != 0 { unsafe { CGEventTapEnable(port as *const _, true); } }
        return CallbackResult::Keep;
    }
    // 1. Ignore events injected by Mouse Insight
    let user_data = event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA);
    if user_data == EXTRA_INFO as i64 {
        return CallbackResult::Keep;
    }

    let Some(eng) = ENGINE.get() else {
        return CallbackResult::Keep;
    };

    // 2. Handle Keyboard events (for recording and physical key tracking)
    match etype {
        CGEventType::KeyDown | CGEventType::KeyUp | CGEventType::FlagsChanged => {
            let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let down = match etype {
                CGEventType::KeyDown => true,
                CGEventType::KeyUp => false,
                CGEventType::FlagsChanged => {
                    // Aggregate flags cannot distinguish releasing one Shift
                    // while the other Shift remains held.
                    unsafe { CGEventSourceKeyState(CGEventSourceStateID::HIDSystemState, keycode) }
                }
                _ => false,
            };

            // Maintain physical key set
            {
                let mut held = physical_down_set().write();
                if down {
                    held.insert(keycode as u32);
                } else {
                    held.remove(&(keycode as u32));
                }
            }

            // Emergency stop: F13, or Ctrl+Option+Cmd+P. Not swallowed — the
            // event keeps flowing so the chord also reaches other apps.
            if down
                && (keycode == kVK_F13
                    || (keycode == kVK_ANSI_P
                        && event.get_flags().contains(
                            CGEventFlags::CGEventFlagControl
                                | CGEventFlags::CGEventFlagAlternate
                                | CGEventFlags::CGEventFlagCommand,
                        )))
            {
                crate::engine::request_emergency_stop(true);
            }

            // A key-up whose down was swallowed during recording must stay
            // swallowed, even after recording ended.
            if !down && eng.swallowed_keys.write().remove(&(keycode as u32)) {
                if eng.recording.load(Ordering::Relaxed) {
                    let _ = eng.recorder.write().on_key(keycode as u32, false);
                }
                return CallbackResult::Drop;
            }

            // Recording mode handling
            if eng.recording.load(Ordering::Relaxed) {
                if keycode == kVK_Tab { return CallbackResult::Keep; }
                if down && keycode == kVK_Escape {
                    eng.swallowed_keys.write().insert(keycode as u32);
                    eng.recording.store(false, Ordering::Relaxed);
                    let _ = eng.cmd_tx.send(InputCmd::RecordCancel);
                    return CallbackResult::Drop;
                }

                // Keys without a token (CapsLock, F21+, media keys) pass
                // through untouched — same as Windows.
                if keycode_to_token(keycode).is_none() {
                    return CallbackResult::Keep;
                }

                if down {
                    eng.swallowed_keys.write().insert(keycode as u32);
                }
                let chord = eng.recorder.write().on_key(keycode as u32, down);
                if !chord.is_empty() {
                    let _ = eng.cmd_tx.send(InputCmd::Record(chord));
                }
                return CallbackResult::Drop;
            }

            return CallbackResult::Keep;
        }
        _ => {}
    }

    // 3. Handle Mouse events
    let (button, down) = match etype {
        CGEventType::OtherMouseDown => {
            let num = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
            match num {
                2 => (MouseButton::Middle, true),
                3 => (MouseButton::XButton1, true),
                4 => (MouseButton::XButton2, true),
                _ => return CallbackResult::Keep,
            }
        }
        CGEventType::OtherMouseUp => {
            let num = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
            match num {
                2 => (MouseButton::Middle, false),
                3 => (MouseButton::XButton1, false),
                4 => (MouseButton::XButton2, false),
                _ => return CallbackResult::Keep,
            }
        }
        CGEventType::LeftMouseDown => (MouseButton::Left, true),
        CGEventType::LeftMouseUp => (MouseButton::Left, false),
        CGEventType::RightMouseDown => (MouseButton::Right, true),
        CGEventType::RightMouseUp => (MouseButton::Right, false),
        CGEventType::ScrollWheel => {
            let continuous = event
                .get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_IS_CONTINUOUS)
                != 0;
            let d1 = event
                .get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1);
            if d1 == 0 {
                return CallbackResult::Keep; // pure horizontal or empty event
            }
            static SCROLL_ACC: AtomicI64 = AtomicI64::new(0);
            static LAST_SCROLL_STEP: AtomicU64 = AtomicU64::new(0);
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            match crate::engine::scroll_step(continuous, d1, &SCROLL_ACC, now_ms, &LAST_SCROLL_STEP) {
                Some(b) => (b, true),
                None => return CallbackResult::Keep,
            }
        }
        _ => return CallbackResult::Keep,
    };

    // Primary buttons never consume the listen session — they keep their
    // system behavior; the frontend hints via the telemetry pulse.
    let captured = down && !button.is_primary() && eng.listening.swap(false, Ordering::Relaxed);
    if captured {
        let _ = eng
            .cmd_tx
            .send(InputCmd::ListenCaptured(button.as_str().to_string()));
    }

    let paused = eng.paused.load(Ordering::Relaxed);
    let compiled = eng.compiled.load_full();
    let action_opt = compiled.get(button).cloned();
    let is_mapped = action_opt.is_some() && !paused && !captured && !eng.recording.load(Ordering::Relaxed);
    let mut queued = false;

    if is_mapped {
        queued = eng.enqueue_edge(InputCmd::MouseEdge {
            button,
            down,
            action: action_opt,
            generation: compiled.generation,
            at: Instant::now(),
        });
    } else if !down && !button.is_primary() && !button.is_wheel() {
        let bit = 1u32 << button as u32;
        if eng.swallowed_buttons.load(Ordering::Relaxed) & bit != 0 {
            let _ = eng.enqueue_edge(InputCmd::MouseEdge {
                button,
                down: false,
                action: None,
                generation: compiled.generation,
                at: Instant::now(),
            });
        }
    }

    let mut swallow = crate::engine::decide_swallow(button, down, is_mapped, captured, queued);
    if !button.is_primary() && !button.is_wheel() {
        let bit = 1u32 << button as u32;
        if down && swallow {
            eng.swallowed_buttons.fetch_or(bit, Ordering::Relaxed);
        } else if !down {
            swallow = eng.swallowed_buttons.fetch_and(!bit, Ordering::Relaxed) & bit != 0;
        }
    }

    let need_telemetry =
        eng.window_visible.load(Ordering::Relaxed) || eng.listening.load(Ordering::Relaxed);

    if need_telemetry {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let pulse = Pulse {
            button: button.as_str().to_string(),
            down,
            t,
            swallowed: swallow,
        };
        *eng.last.write() = Some(pulse.clone());
        let _ = eng.telem_tx.try_send(pulse);
    }

    if swallow {
        CallbackResult::Drop
    } else {
        CallbackResult::Keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macos_key_spec_mapping() {
        assert_eq!(key_spec("Command").unwrap().vk.0, kVK_Command);
        assert_eq!(key_spec("Cmd").unwrap().vk.0, kVK_Command);
        assert_eq!(key_spec("Option").unwrap().vk.0, kVK_Option);
        assert_eq!(key_spec("Alt").unwrap().vk.0, kVK_Option);
        assert_eq!(key_spec("Control").unwrap().vk.0, kVK_Control);
        assert_eq!(key_spec("Shift").unwrap().vk.0, kVK_Shift);
        assert_eq!(key_spec("Return").unwrap().vk.0, kVK_Return);
        assert_eq!(key_spec("Enter").unwrap().vk.0, kVK_Return);
        assert_eq!(key_spec("Space").unwrap().vk.0, kVK_Space);
        assert_eq!(key_spec("Escape").unwrap().vk.0, kVK_Escape);
        assert_eq!(key_spec("A").unwrap().vk.0, kVK_ANSI_A);
        assert_eq!(key_spec("F1").unwrap().vk.0, kVK_F1);
        assert_eq!(key_spec("ArrowLeft").unwrap().vk.0, kVK_LeftArrow);
    }

    #[test]
    fn test_macos_keycode_to_token() {
        assert_eq!(keycode_to_token(kVK_Command).as_deref(), Some("LWin"));
        assert_eq!(keycode_to_token(kVK_Option).as_deref(), Some("LAlt"));
        assert_eq!(keycode_to_token(kVK_Control).as_deref(), Some("LControl"));
        assert_eq!(keycode_to_token(kVK_Shift).as_deref(), Some("LShift"));
        assert_eq!(keycode_to_token(kVK_Return).as_deref(), Some("Enter"));
        assert_eq!(keycode_to_token(kVK_Space).as_deref(), Some("Space"));
        assert_eq!(keycode_to_token(kVK_ANSI_A).as_deref(), Some("A"));
    }

    #[test]
    fn test_is_modifier_keycode() {
        assert!(is_modifier_keycode(kVK_Command));
        assert!(is_modifier_keycode(kVK_Option));
        assert!(is_modifier_keycode(kVK_Control));
        assert!(is_modifier_keycode(kVK_Shift));
        assert!(!is_modifier_keycode(kVK_Return));
        assert!(!is_modifier_keycode(kVK_ANSI_A));
    }
    #[test]
    fn right_option_round_trips_without_left_alias() {
        let right = key_spec("RAlt").unwrap();
        let left = key_spec("LAlt").unwrap();
        assert_eq!(right.vk.0, kVK_RightOption);
        assert_ne!(right.vk.0, left.vk.0);
        assert_eq!(keycode_to_token(right.vk.0).as_deref(), Some("RAlt"));
    }

    #[test]
    fn both_option_sides_share_flag_but_have_distinct_ownership() {
        let mut held = HashSet::from([kVK_Option, kVK_RightOption]);
        held.remove(&kVK_RightOption);
        let flags = held.into_iter().fold(CGEventFlags::empty(), |f, key| f | modifier_flag(key));
        assert!(flags.contains(CGEventFlags::CGEventFlagAlternate));
    }

    #[test]
    fn injected_modifiers_preserve_sides_and_caps_lock() {
        let pairs = [(kVK_Command, kVK_RightCommand, 0x08, 0x10),
            (kVK_Option, kVK_RightOption, 0x20, 0x40),
            (kVK_Control, kVK_RightControl, 0x01, 0x2000),
            (kVK_Shift, kVK_RightShift, 0x02, 0x04)];
        for (left, right, left_mask, right_mask) in pairs {
            let mut held = HashSet::from([left, right]);
            let both = injected_flags(CGEventFlags::CGEventFlagAlphaShift, &held);
            assert_ne!(both.bits() & left_mask, 0);
            assert_ne!(both.bits() & right_mask, 0);
            held.remove(&right);
            let released = injected_flags(CGEventFlags::CGEventFlagAlphaShift, &held);
            assert_ne!(released.bits() & left_mask, 0);
            assert_eq!(released.bits() & right_mask, 0);
            assert!(released.contains(CGEventFlags::CGEventFlagAlphaShift));
            assert_eq!(released.bits() & 0x80, 0);
        }
    }

}

