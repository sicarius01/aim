//! 색상 팔레트(VSCode 다크 그레이) + GDI 헬퍼.
use winsafe::{COLORREF, HBRUSH};

pub const C_PANEL: (u8, u8, u8) = (37, 37, 38); // #252526  사이드바
pub const C_EDITOR: (u8, u8, u8) = (30, 30, 30); // #1E1E1E  에임 영역
pub const C_EDIT_BG: (u8, u8, u8) = (60, 60, 60); // #3C3C3C  입력창
pub const C_TEXT: (u8, u8, u8) = (212, 212, 212); // #D4D4D4
pub const C_TARGET: (u8, u8, u8) = (86, 156, 214); // #569CD6
pub const C_TARGET_EDGE: (u8, u8, u8) = (40, 78, 120);
pub const C_TARGET_HOT: (u8, u8, u8) = (78, 201, 176); // 추적 중 링 안(틸)
pub const C_CURSOR: (u8, u8, u8) = (64, 200, 120); // 초록 링
pub const C_HIT: (u8, u8, u8) = (78, 201, 120);
pub const C_MISS: (u8, u8, u8) = (224, 108, 108);
pub const C_SPARK: (u8, u8, u8) = (120, 160, 90);

pub const CURSOR_RADIUS: i32 = 9;
pub const CURSOR_THICK: i32 = 2;

#[inline]
pub fn rgb(c: (u8, u8, u8)) -> COLORREF {
    COLORREF::from_rgb(c.0, c.1, c.2)
}

/// 앱 수명 동안 유지되는 솔리드 브러시(클래스 배경/CtlColor용). 프로세스 종료 시 OS가 회수.
pub fn leak_brush(c: (u8, u8, u8)) -> HBRUSH {
    let g = HBRUSH::CreateSolidBrush(rgb(c)).unwrap();
    let h = unsafe { g.raw_copy() };
    std::mem::forget(g);
    h
}
