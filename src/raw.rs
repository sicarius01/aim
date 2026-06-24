//! user32 / dwmapi 직접 FFI (winsafe 0.0.27 미지원 항목).
//!
//! winsafe에는 raw mouse input, PostMessage, DwmFlush, GetSystemMetrics 래퍼가
//! 없어 여기서 직접 선언한다.
#![allow(non_snake_case)]

use std::ffi::c_void;

pub const RID_INPUT: u32 = 0x1000_0003;
pub const RIM_TYPEMOUSE: u32 = 0;
pub const MOUSE_MOVE_ABSOLUTE: u16 = 0x0001;
pub const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
pub const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

#[repr(C)]
pub struct RawInputDevice {
    pub us_usage_page: u16,
    pub us_usage: u16,
    pub dw_flags: u32,
    pub hwnd_target: *mut c_void,
}

#[repr(C)]
pub struct RawInputHeader {
    pub dw_type: u32,
    pub dw_size: u32,
    pub h_device: *mut c_void,
    pub w_param: usize,
}

// RAWMOUSE: usFlags 뒤에 4바이트 정렬 union이 오므로 2바이트 패딩 필요.
#[repr(C)]
pub struct RawMouse {
    pub us_flags: u16,
    pub _pad: u16,
    pub us_button_flags: u16,
    pub us_button_data: u16,
    pub ul_raw_buttons: u32,
    pub l_last_x: i32,
    pub l_last_y: i32,
    pub ul_extra: u32,
}

#[repr(C)]
pub struct RawInput {
    pub header: RawInputHeader,
    pub mouse: RawMouse,
}

// 페이서 스레드 → UI 스레드 프레임 신호 (WM_APP 범위, 일반 우선순위라 굶지 않음)
pub const WM_APP_FRAME: u32 = 0x8000 + 1;

pub const SM_CXSCREEN: i32 = 0; // 주 모니터 가로 px
pub const SM_CYSCREEN: i32 = 1; // 주 모니터 세로 px

#[link(name = "user32")]
extern "system" {
    pub fn RegisterRawInputDevices(p: *const RawInputDevice, num: u32, size: u32) -> i32;
    pub fn GetRawInputData(
        h: *mut c_void,
        cmd: u32,
        data: *mut c_void,
        size: *mut u32,
        hdr_size: u32,
    ) -> u32;
    pub fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
    pub fn GetSystemMetrics(index: i32) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    // 다음 DWM 컴포지션(수직동기)까지 블록. 주사율마다 1회 반환 → vsync 페이싱.
    pub fn DwmFlush() -> i32; // HRESULT, S_OK == 0
}
