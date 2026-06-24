// Aim Trainer — winsafe(Win32) 기반 간단한 에임 연습 프로그램
//
// 구성:
//  - 좌측: 에임 영역(커스텀 GDI 페인팅). 랜덤 위치에 타깃(원)이 주기적으로 생성/소멸.
//          연습 중 마우스 커서는 raw input으로 직접 구동되는 "초록 빈 링"으로 표시.
//  - 우측: VSCode 다크 그레이 톤의 설정/기록 패널.
//
// 감도(eDPI): eDPI = DPI × 감도. raw mouse input(가속 무시)으로 커서를 직접 움직여
// 실제 게임 감도 연습이 가능. (winsafe 0.0.27에는 raw input 래퍼가 없어 user32.dll을 직접 FFI)

#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winsafe::{self as w, co, gui, prelude::*};
use winsafe::{COLORREF, HBRUSH, HPEN, POINT, RECT, SIZE};

// ───────────────────────────── 라이브러리 외부: Raw Input (user32.dll 직접 FFI) ─────────────────────────────
mod raw {
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
    }

    #[link(name = "dwmapi")]
    extern "system" {
        // 다음 DWM 컴포지션(수직동기)까지 블록. 주사율마다 1회 반환 → vsync 페이싱.
        pub fn DwmFlush() -> i32; // HRESULT, S_OK == 0
    }
}

// ───────────────────────────── 상수/색상 ─────────────────────────────
// 감도 모델: 게임처럼 "1카운트당 회전 각도(yaw×감도)"를 화면 픽셀로 투영한다.
//   pixels_per_count = sens × yaw × (focal × π/180) × speed_mult
//   focal = (에임폭/2) / tan(FOV/2)  → 화면 중앙 기준 °당 픽셀
// 이렇게 하면 cm/360 이 게임과 동일하게 맞춰져 eDPI 연습이 실제로 의미를 가진다.
const HFOV_DEG: f64 = 103.0; // 수평 시야각(에임 영역이 나타내는 각도) — KovaaK/CS 류 기본값
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;
// 페이서 스레드가 DwmFlush 실패 시 쓰는 폴백 간격(약 144Hz)
const FALLBACK_FRAME: Duration = Duration::from_micros(6_944);

const C_PANEL: (u8, u8, u8) = (37, 37, 38); // #252526  사이드바
const C_EDITOR: (u8, u8, u8) = (30, 30, 30); // #1E1E1E  에디터(에임 영역)
const C_EDIT_BG: (u8, u8, u8) = (60, 60, 60); // #3C3C3C  입력창
const C_TEXT: (u8, u8, u8) = (212, 212, 212); // #D4D4D4
const C_TARGET: (u8, u8, u8) = (86, 156, 214); // #569CD6
const C_TARGET_EDGE: (u8, u8, u8) = (40, 78, 120);
const C_CURSOR: (u8, u8, u8) = (64, 200, 120); // 초록 링

const CURSOR_RADIUS: i32 = 9;
const CURSOR_THICK: i32 = 2;

#[inline]
fn rgb(c: (u8, u8, u8)) -> COLORREF {
    COLORREF::from_rgb(c.0, c.1, c.2)
}

fn leak_brush(c: (u8, u8, u8)) -> HBRUSH {
    let g = HBRUSH::CreateSolidBrush(rgb(c)).unwrap();
    let h = unsafe { g.raw_copy() };
    std::mem::forget(g); // 앱 수명 동안 유지 (프로세스 종료 시 OS가 회수)
    h
}

// ───────────────────────────── 상태 ─────────────────────────────
// 참고: DPI는 시뮬레이션(ppc)에 직접 쓰이지 않는다. 원시 카운트가 이미 하드웨어 DPI를
// 반영하기 때문(게임 eDPI 불변성과 동일). DPI는 eDPI/cm360 표시 계산에만 쓴다.
#[derive(Clone, Copy)]
struct Settings {
    sens: f64,
    yaw: f64, // °/카운트 @ 감도 1.0 (게임 상수: CS/Apex 0.022, 발로란트 0.07, 옵치 0.0066)
    speed: f64,
    radius: i32,
    spawn_ms: u64,
    life_ms: u64,
    max_targets: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sens: 2.0,
            yaw: 0.022,
            speed: 1.0,
            radius: 28,
            spawn_ms: 850,
            life_ms: 1100,
            max_targets: 4,
        }
    }
}

struct Target {
    x: i32,
    y: i32,
    r: i32,
    born: Instant,
}

#[derive(Default)]
struct Stats {
    score: f64,
    hits: u32,
    misclicks: u32,
    expired: u32,
    react_sum: f64,
    react_best: f64,
    acc_sum: f64,
}

struct State {
    settings: Settings,
    running: bool,
    raw_input_ok: bool,
    targets: Vec<Target>,
    cx: f64,
    cy: f64,
    aim_w: i32,
    aim_h: i32,
    last_spawn: Option<Instant>,
    px_per_deg: f64, // 시작 시 에임폭+FOV로 계산
    rng: u64,
    stats: Stats,
    tick: u64,
}

impl State {
    fn new() -> Self {
        // 간단한 시드 (시간 기반)
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
        Self {
            settings: Settings::default(),
            running: false,
            raw_input_ok: false,
            targets: Vec::new(),
            cx: 0.0,
            cy: 0.0,
            aim_w: 1,
            aim_h: 1,
            last_spawn: None,
            px_per_deg: 1.0,
            rng: seed,
            stats: Stats::default(),
            tick: 0,
        }
    }

    fn next_rand(&mut self) -> u64 {
        // xorshift64
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }
}

// ───────────────────────────── 앱 ─────────────────────────────
#[derive(Clone)]
struct App {
    wnd: gui::WindowMain,
    aim: gui::WindowControl,

    e_dpi: gui::Edit,
    e_sens: gui::Edit,
    e_yaw: gui::Edit,
    e_speed: gui::Edit,
    e_radius: gui::Edit,
    e_spawn: gui::Edit,
    e_life: gui::Edit,
    e_max: gui::Edit,

    l_edpi: gui::Label,
    l_cm360: gui::Label,
    btn_start: gui::Button,
    btn_reset: gui::Button,

    l_score: gui::Label,
    l_hits: gui::Label,
    l_acc: gui::Label,
    l_react: gui::Label,
    l_best: gui::Label,
    l_center: gui::Label,
    l_status: gui::Label,

    state: Rc<RefCell<State>>,
    // 페이서 스레드 ↔ UI 스레드 공유 플래그 (스레드 안전)
    session_active: Arc<AtomicBool>, // 연습 진행 중에만 프레임 신호
    alive: Arc<AtomicBool>,          // 앱 종료 시 페이서 스레드 정지
    frame_pending: Arc<AtomicBool>,  // 미처리 프레임 1개로 제한(큐 적체 방지)
}

impl App {
    fn new() -> Self {
        let panel_brush = leak_brush(C_PANEL);
        let editor_brush = leak_brush(C_EDITOR);

        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            title: "Notes",
            size: (1024, 706),
            class_bg_brush: gui::Brush::Handle(panel_brush),
            style: co::WS::CAPTION
                | co::WS::SYSMENU
                | co::WS::MINIMIZEBOX
                | co::WS::CLIPCHILDREN
                | co::WS::BORDER
                | co::WS::VISIBLE,
            ..Default::default()
        });

        let aim = gui::WindowControl::new(
            &wnd,
            gui::WindowControlOpts {
                position: (10, 8),
                size: (694, 652),
                class_bg_brush: gui::Brush::Handle(editor_brush),
                class_cursor: gui::Cursor::Idc(co::IDC::ARROW),
                ..Default::default()
            },
        );

        // 패널 레이아웃 좌표
        const LX: i32 = 716; // 라벨 x
        const VX: i32 = 904; // 값/입력 x
        const VW: i32 = 100; // 값/입력 폭
        let lbl = |parent: &gui::WindowMain, text: &str, x: i32, y: i32| {
            gui::Label::new(
                parent,
                gui::LabelOpts {
                    text: &text.to_owned(),
                    position: (x, y),
                    ..Default::default()
                },
            )
        };
        let val = |parent: &gui::WindowMain, text: &str, y: i32| {
            gui::Label::new(
                parent,
                gui::LabelOpts {
                    text: &text.to_owned(),
                    position: (VX, y),
                    size: (180, 20),
                    ..Default::default()
                },
            )
        };
        let edit = |parent: &gui::WindowMain, text: &str, y: i32| {
            gui::Edit::new(
                parent,
                gui::EditOpts {
                    text: &text.to_owned(),
                    position: (VX, y - 2),
                    width: VW,
                    height: 22,
                    ..Default::default()
                },
            )
        };

        let _hdr1 = lbl(&wnd, "설정", LX, 12);
        let _l1 = lbl(&wnd, "마우스 DPI", LX, 42);
        let e_dpi = edit(&wnd, "800", 42);
        let _l2 = lbl(&wnd, "감도 (sensitivity)", LX, 70);
        let e_sens = edit(&wnd, "2.0", 70);
        let _l3 = lbl(&wnd, "게임 yaw (°/카운트)", LX, 98);
        let e_yaw = edit(&wnd, "0.022", 98);
        let _l4 = lbl(&wnd, "eDPI (DPI × 감도)", LX, 126);
        let l_edpi = val(&wnd, "1600", 126);
        let _l5 = lbl(&wnd, "cm/360 (≈게임 체감)", LX, 152);
        let l_cm360 = val(&wnd, "—", 152);
        let _l6 = lbl(&wnd, "커서 속도 배율", LX, 180);
        let e_speed = edit(&wnd, "1.0", 180);

        let _l7 = lbl(&wnd, "타깃 크기 (반지름 px)", LX, 212);
        let e_radius = edit(&wnd, "28", 212);
        let _l8 = lbl(&wnd, "생성 주기 (ms)", LX, 240);
        let e_spawn = edit(&wnd, "850", 240);
        let _l9 = lbl(&wnd, "타깃 수명 (ms)", LX, 268);
        let e_life = edit(&wnd, "1100", 268);
        let _l10 = lbl(&wnd, "최대 동시 타깃", LX, 296);
        let e_max = edit(&wnd, "4", 296);

        let btn_start = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "시작 (F2)",
                position: (LX, 330),
                width: 170,
                height: 30,
                ..Default::default()
            },
        );
        let btn_reset = gui::Button::new(
            &wnd,
            gui::ButtonOpts {
                text: "기록 초기화",
                position: (LX + 184, 330),
                width: 100,
                height: 30,
                ..Default::default()
            },
        );

        let _hdr2 = lbl(&wnd, "기록", LX, 374);
        let _s1 = lbl(&wnd, "점수", LX, 400);
        let l_score = val(&wnd, "0", 400);
        let _s2 = lbl(&wnd, "명중 (놓침·빗맞힘)", LX, 426);
        let l_hits = val(&wnd, "0", 426);
        let _s3 = lbl(&wnd, "클릭 정확도", LX, 452);
        let l_acc = val(&wnd, "0.0%", 452);
        let _s4 = lbl(&wnd, "평균 반응속도", LX, 478);
        let l_react = val(&wnd, "0 ms", 478);
        let _s5 = lbl(&wnd, "최고 반응속도", LX, 504);
        let l_best = val(&wnd, "0 ms", 504);
        let _s6 = lbl(&wnd, "평균 중앙 정확도", LX, 530);
        let l_center = val(&wnd, "0.0%", 530);

        let l_status = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "정지됨. [시작] 또는 F2.  진행 중엔 ESC로 정지.",
                position: (LX, 562),
                size: (300, 32),
                ..Default::default()
            },
        );
        let _hint = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "초록 링으로 조준 후 좌클릭. cm/360 을 본인 게임 값에 맞추면 감도가 동일해집니다 \
                       (게임 yaw: CS/Apex 0.022, 발로란트 0.07, 옵치 0.0066). Windows '포인터 정확도 향상'은 꺼두세요.",
                position: (LX, 598),
                size: (300, 72),
                ..Default::default()
            },
        );

        Self {
            wnd,
            aim,
            e_dpi,
            e_sens,
            e_yaw,
            e_speed,
            e_radius,
            e_spawn,
            e_life,
            e_max,
            l_edpi,
            l_cm360,
            btn_start,
            btn_reset,
            l_score,
            l_hits,
            l_acc,
            l_react,
            l_best,
            l_center,
            l_status,
            state: Rc::new(RefCell::new(State::new())),
            session_active: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(true)),
            frame_pending: Arc::new(AtomicBool::new(false)),
        }
    }

    fn run(&self) -> w::AnyResult<i32> {
        self.events();
        self.wnd.run_main(None)
    }

    // ── 이벤트 등록 ──
    fn events(&self) {
        let panel_brush = leak_brush(C_PANEL);
        let edit_brush = leak_brush(C_EDIT_BG);

        // 다크 테마: 정적 라벨
        self.wnd.on().wm_ctl_color_static(move |p| {
            p.hdc.SetTextColor(rgb(C_TEXT))?;
            p.hdc.SetBkColor(rgb(C_PANEL))?;
            Ok(unsafe { panel_brush.raw_copy() })
        });
        // 다크 테마: 입력창
        self.wnd.on().wm_ctl_color_edit(move |p| {
            p.hdc.SetTextColor(rgb(C_TEXT))?;
            p.hdc.SetBkColor(rgb(C_EDIT_BG))?;
            Ok(unsafe { edit_brush.raw_copy() })
        });

        // 메인 윈도우 생성: raw input 등록 + 타이머 시작
        let me = self.clone();
        self.wnd.on().wm_create(move |_| {
            let hwnd_ptr = me.wnd.hwnd().ptr();
            let dev = raw::RawInputDevice {
                us_usage_page: raw::HID_USAGE_PAGE_GENERIC,
                us_usage: raw::HID_USAGE_GENERIC_MOUSE,
                dw_flags: 0, // 포그라운드일 때만 수신
                hwnd_target: hwnd_ptr,
            };
            let ok = unsafe {
                raw::RegisterRawInputDevices(
                    &dev,
                    1,
                    std::mem::size_of::<raw::RawInputDevice>() as u32,
                )
            } != 0;
            me.state.borrow_mut().raw_input_ok = ok;
            if !ok {
                me.set_status("Raw input 등록 실패 — 마우스 추적 불가. 앱을 재시작해 보세요.");
            }
            me.update_edpi();

            // vsync 페이서 스레드 기동: DwmFlush로 vblank까지 잠들었다가(=busy-wait 아님)
            // 매 vblank마다 UI 스레드에 프레임 메시지를 보낸다. → 주사율 그대로 렌더, CPU 거의 0.
            let hwnd_usize = hwnd_ptr as usize;
            let alive = me.alive.clone();
            let active = me.session_active.clone();
            let pending = me.frame_pending.clone();
            std::thread::spawn(move || {
                while alive.load(Ordering::Relaxed) {
                    let hr = unsafe { raw::DwmFlush() };
                    if hr != 0 {
                        // DWM 사용 불가 시 폴백(약 144Hz)
                        std::thread::sleep(FALLBACK_FRAME);
                    }
                    if !alive.load(Ordering::Relaxed) {
                        break;
                    }
                    // 진행 중이고 미처리 프레임이 없을 때만 한 장 요청(큐 적체 방지)
                    if active.load(Ordering::Relaxed)
                        && !pending.swap(true, Ordering::AcqRel)
                    {
                        unsafe {
                            raw::PostMessageW(
                                hwnd_usize as *mut c_void,
                                raw::WM_APP_FRAME,
                                0,
                                0,
                            );
                        }
                    }
                }
            });
            Ok(0)
        });

        // eDPI 실시간 갱신
        let me = self.clone();
        self.e_dpi.on().en_change(move || {
            me.update_edpi();
            Ok(())
        });
        let me = self.clone();
        self.e_sens.on().en_change(move || {
            me.update_edpi();
            Ok(())
        });
        let me = self.clone();
        self.e_yaw.on().en_change(move || {
            me.update_edpi();
            Ok(())
        });

        // 시작/정지 버튼
        let me = self.clone();
        self.btn_start.on().bn_clicked(move || {
            me.toggle();
            Ok(())
        });
        // 기록 초기화
        let me = self.clone();
        self.btn_reset.on().bn_clicked(move || {
            {
                let mut st = me.state.borrow_mut();
                st.stats = Stats::default();
            }
            me.update_stats();
            Ok(())
        });

        // Raw input (마우스 이동) → 가상 커서 갱신.
        // 처리 후 DefWindowProc를 호출해 시스템이 raw input 버퍼를 정리하게 함.
        let me = self.clone();
        self.wnd.on().wm(co::WM::INPUT, move |p| {
            me.on_raw_input(p.lparam);
            let res = unsafe { me.wnd.hwnd().DefWindowProc(p) };
            Ok(res)
        });

        // vsync 프레임 신호(페이서 스레드가 매 vblank에 보냄) → 한 프레임 진행+렌더.
        // WM_APP 범위라 일반 우선순위 → WM_INPUT 폭주에도 굶지 않는다.
        let me = self.clone();
        let wm_frame = unsafe { co::WM::from_raw(raw::WM_APP_FRAME) };
        self.wnd.on().wm(wm_frame, move |_| {
            me.frame_pending.store(false, Ordering::Release);
            me.frame();
            Ok(0)
        });

        // 앱 종료 시 페이서 스레드 정지
        let me = self.clone();
        self.wnd.on().wm_destroy(move || {
            me.alive.store(false, Ordering::Release);
            Ok(())
        });

        // 에임 영역 페인팅
        let me = self.clone();
        self.aim.on().wm_paint(move || {
            me.on_paint()?;
            Ok(())
        });
        // 깜빡임 방지: 기본 배경 지우기 무시 (전체를 직접 그림)
        self.aim.on().wm_erase_bkgnd(|_| Ok(1));

        // 클릭(좌클릭) → 명중 판정 (가상 커서 위치 사용)
        let me = self.clone();
        self.aim.on().wm_l_button_down(move |_| {
            me.on_click();
            Ok(())
        });

        // 키 입력: ESC=정지, F2=토글
        let me = self.clone();
        self.aim.on().wm_key_down(move |k| {
            if k.vkey_code == co::VK::ESCAPE {
                me.stop();
            } else if k.vkey_code == co::VK::F2 {
                me.toggle();
            }
            Ok(())
        });

        // 포커스 상실(Alt-Tab 등) 시 안전하게 정지 → 커서 복구.
        // 단, 시작/정지 버튼으로 포커스가 넘어가는 경우는 제외 (버튼의 토글이 상태를 제어하도록).
        let me = self.clone();
        self.aim.on().wm_kill_focus(move |p| {
            let to_button = p
                .hwnd
                .as_ref()
                .map(|h| h.ptr() == me.btn_start.hwnd().ptr())
                .unwrap_or(false);
            if !to_button {
                me.stop();
            }
            Ok(())
        });
    }

    // ── eDPI / cm·360 표시 ──
    fn update_edpi(&self) {
        // 실제 시뮬레이션에 쓰이는 값(클램프/기본값 포함)과 동일하게 표시
        let dpi = resolve_dpi(&self.e_dpi);
        let sens = resolve_sens(&self.e_sens);
        let yaw = resolve_yaw(&self.e_yaw);
        let _ = self.l_edpi.hwnd().SetWindowText(&format!("{:.0}", dpi * sens));
        // cm/360 = (360 × 2.54) / (DPI × 감도 × yaw)
        let denom = dpi * sens * yaw;
        let cm360 = if denom > 0.0 { 360.0 * 2.54 / denom } else { 0.0 };
        let _ = self
            .l_cm360
            .hwnd()
            .SetWindowText(&format!("{:.1} cm", cm360));
    }

    // ── 시작/정지 ──
    fn toggle(&self) {
        let running = self.state.borrow().running;
        if running {
            self.stop();
        } else {
            self.start();
        }
    }

    fn start(&self) {
        {
            let st = self.state.borrow();
            if st.running {
                return;
            }
            if !st.raw_input_ok {
                drop(st);
                self.set_status("Raw input 사용 불가 — 시작할 수 없습니다. 앱을 재시작해 보세요.");
                return;
            }
        }
        let rc = self
            .aim
            .hwnd()
            .GetClientRect()
            .unwrap_or(RECT { left: 0, top: 0, right: 1, bottom: 1 });

        let settings = Settings {
            sens: resolve_sens(&self.e_sens),
            yaw: resolve_yaw(&self.e_yaw),
            speed: parse_f64(&self.e_speed).unwrap_or(1.0).clamp(0.05, 20.0),
            radius: parse_f64(&self.e_radius).unwrap_or(28.0).clamp(6.0, 120.0) as i32,
            spawn_ms: parse_f64(&self.e_spawn).unwrap_or(850.0).clamp(50.0, 10000.0) as u64,
            life_ms: parse_f64(&self.e_life).unwrap_or(1100.0).clamp(100.0, 20000.0) as u64,
            max_targets: parse_f64(&self.e_max).unwrap_or(4.0).clamp(1.0, 50.0) as usize,
        };

        // focal = (에임폭/2) / tan(FOV/2) → 화면 중앙 기준 픽셀/°
        let focal = (rc.right as f64 / 2.0) / (HFOV_DEG * 0.5 * DEG2RAD).tan();
        let px_per_deg = focal * DEG2RAD;

        {
            let mut st = self.state.borrow_mut();
            st.settings = settings;
            st.aim_w = rc.right;
            st.aim_h = rc.bottom;
            st.px_per_deg = px_per_deg;
            st.cx = rc.right as f64 / 2.0;
            st.cy = rc.bottom as f64 / 2.0;
            st.targets.clear();
            st.last_spawn = None;
            st.running = true;
        }
        self.session_active.store(true, Ordering::Release);

        // 커서 숨김 + 에임 영역에 가둠
        w::ShowCursor(false);
        if let Ok(screen) = self.aim.hwnd().ClientToScreenRc(rc) {
            let _ = w::ClipCursor(Some(&screen));
        }
        if let Ok(center) = self.aim.hwnd().ClientToScreen(POINT {
            x: rc.right / 2,
            y: rc.bottom / 2,
        }) {
            let _ = w::SetCursorPos(center.x, center.y);
        }
        let _ = self.aim.hwnd().SetFocus();
        let _ = self.btn_start.hwnd().SetWindowText("정지 (ESC)");
        self.set_status("진행 중 — 초록 링으로 조준해 좌클릭. ESC로 정지.");
        let _ = self.aim.hwnd().InvalidateRect(None, false);
    }

    fn stop(&self) {
        if !self.state.borrow().running {
            return;
        }
        {
            let mut st = self.state.borrow_mut();
            st.running = false;
            st.targets.clear();
        }
        self.session_active.store(false, Ordering::Release);
        let _ = w::ClipCursor(None);
        w::ShowCursor(true);
        let _ = self.btn_start.hwnd().SetWindowText("시작 (F2)");
        self.set_status("정지됨. [시작] 또는 F2.");
        let _ = self.aim.hwnd().InvalidateRect(None, true);
    }

    // ── Raw input 처리 ──
    // WM_INPUT은 마우스 HID 리포트마다 1개씩 전달되며 WM_MOUSEMOVE처럼 합쳐지지 않는다.
    // 즉 모든 이동 이벤트를 받는다. 여기서 가상 커서를 누적시키고, 곧바로 pump()로
    // (스로틀된) 동기 리페인트를 실행해 빠르게 움직여도 커서가 끊기지 않게 한다.
    fn on_raw_input(&self, lparam: isize) {
        // 1) raw 델타 읽기 (상태 borrow 없이)
        let hri = lparam as *mut c_void;
        let hdr = std::mem::size_of::<raw::RawInputHeader>() as u32;
        let mut ri_buf = std::mem::MaybeUninit::<raw::RawInput>::uninit();
        let mut size = std::mem::size_of::<raw::RawInput>() as u32;
        let got = unsafe {
            raw::GetRawInputData(
                hri,
                raw::RID_INPUT,
                ri_buf.as_mut_ptr() as *mut c_void,
                &mut size,
                hdr,
            )
        };
        if got == u32::MAX || got == 0 {
            return;
        }
        let ri = unsafe { &*ri_buf.as_ptr() };
        if ri.header.dw_type != raw::RIM_TYPEMOUSE
            || ri.mouse.us_flags & raw::MOUSE_MOVE_ABSOLUTE != 0
        {
            return;
        }
        let dx = ri.mouse.l_last_x as f64;
        let dy = ri.mouse.l_last_y as f64;

        // 2) 가상 커서 누적만 (렌더는 vsync 프레임이 담당 → 입력은 빠르게 소화)
        let mut st = self.state.borrow_mut();
        if !st.running {
            return;
        }
        if dx != 0.0 || dy != 0.0 {
            // pixels/count = 감도 × yaw(°/카운트) × (focal·π/180) × 배율
            let ppc = st.settings.sens * st.settings.yaw * st.px_per_deg * st.settings.speed;
            let (w_, h_) = (st.aim_w as f64, st.aim_h as f64);
            st.cx = (st.cx + dx * ppc).clamp(0.0, w_);
            st.cy = (st.cy + dy * ppc).clamp(0.0, h_);
        }
    }

    // 한 프레임: 타깃 생성/소멸 전진 + 렌더. vsync 페이서가 매 vblank에 호출한다.
    fn frame(&self) {
        let now = Instant::now();
        let mut do_stats = false;
        {
            let mut st = self.state.borrow_mut();
            if !st.running {
                return;
            }
            // 소멸
            let life = st.settings.life_ms as u128;
            let before = st.targets.len();
            st.targets.retain(|t| t.born.elapsed().as_millis() < life);
            let expired = before - st.targets.len();
            if expired > 0 {
                st.stats.expired += expired as u32;
                do_stats = true;
            }
            // 생성
            let need = match st.last_spawn {
                Some(ls) => ls.elapsed().as_millis() >= st.settings.spawn_ms as u128,
                None => true,
            };
            if need && st.targets.len() < st.settings.max_targets {
                let r = st.settings.radius;
                let margin = r + 4;
                let span_w = (st.aim_w - 2 * margin).max(1) as u64;
                let span_h = (st.aim_h - 2 * margin).max(1) as u64;
                let rx = margin + (st.next_rand() % span_w) as i32;
                let ry = margin + (st.next_rand() % span_h) as i32;
                st.targets.push(Target { x: rx, y: ry, r, born: now });
                st.last_spawn = Some(now);
            }
            st.tick += 1;
            if st.tick % 30 == 0 {
                do_stats = true;
            }
        }
        // 동기 렌더 (큐를 거치지 않으므로 WM_INPUT 폭주에도 굶지 않음)
        let _ = self.aim.hwnd().InvalidateRect(None, false);
        let _ = self.aim.hwnd().UpdateWindow();
        if do_stats {
            self.update_stats();
        }
    }

    // ── 클릭 판정 ──
    fn on_click(&self) {
        {
            let mut st = self.state.borrow_mut();
            if !st.running {
                return;
            }
            let (cx, cy) = (st.cx, st.cy);
            let mut hit: Option<usize> = None;
            let mut best_d = f64::MAX;
            for (i, t) in st.targets.iter().enumerate() {
                let d = ((t.x as f64 - cx).powi(2) + (t.y as f64 - cy).powi(2)).sqrt();
                if d <= t.r as f64 && d < best_d {
                    best_d = d;
                    hit = Some(i);
                }
            }
            if let Some(i) = hit {
                let t = st.targets.remove(i);
                let react = t.born.elapsed().as_secs_f64() * 1000.0;
                let acc = (1.0 - best_d / t.r as f64).clamp(0.0, 1.0);
                let life = st.settings.life_ms as f64;
                let speed_factor = (1.0 - react / life).clamp(0.0, 1.0);
                let points = 100.0 * (0.25 + 0.75 * acc) * (0.35 + 0.65 * speed_factor);
                st.stats.score += points;
                st.stats.hits += 1;
                st.stats.react_sum += react;
                st.stats.acc_sum += acc;
                if st.stats.react_best == 0.0 || react < st.stats.react_best {
                    st.stats.react_best = react;
                }
            } else {
                st.stats.misclicks += 1;
                st.stats.score = (st.stats.score - 10.0).max(0.0);
            }
        }
        self.update_stats();
        let _ = self.aim.hwnd().InvalidateRect(None, false);
        let _ = self.aim.hwnd().UpdateWindow(); // 클릭 즉시 반영
    }

    // ── 페인팅 ──
    fn on_paint(&self) -> w::AnyResult<()> {
        let st = self.state.borrow();
        let hdc = self.aim.hwnd().BeginPaint()?;
        let rc = self.aim.hwnd().GetClientRect()?;
        let (w_, h_) = (rc.right, rc.bottom);

        let mem = hdc.CreateCompatibleDC()?;
        let bmp = hdc.CreateCompatibleBitmap(w_, h_)?;
        let _old_bmp = mem.SelectObject(&*bmp)?;

        // 배경
        let bg = HBRUSH::CreateSolidBrush(rgb(C_EDITOR))?;
        mem.FillRect(rc, &bg)?;

        // 타깃들
        {
            let tb = HBRUSH::CreateSolidBrush(rgb(C_TARGET))?;
            let tp = HPEN::CreatePen(co::PS::SOLID, 1, rgb(C_TARGET_EDGE))?;
            let _ob = mem.SelectObject(&*tb)?;
            let _op = mem.SelectObject(&*tp)?;
            for t in &st.targets {
                mem.Ellipse(RECT {
                    left: t.x - t.r,
                    top: t.y - t.r,
                    right: t.x + t.r,
                    bottom: t.y + t.r,
                })?;
            }
        }

        // 커서(빈 초록 링) — 진행 중에만
        if st.running {
            let null_b = HBRUSH::GetStockObject(co::STOCK_BRUSH::NULL)?;
            let cp = HPEN::CreatePen(co::PS::SOLID, CURSOR_THICK, rgb(C_CURSOR))?;
            let _ob = mem.SelectObject(&null_b)?;
            let _op = mem.SelectObject(&*cp)?;
            let (cx, cy) = (st.cx as i32, st.cy as i32);
            mem.Ellipse(RECT {
                left: cx - CURSOR_RADIUS,
                top: cy - CURSOR_RADIUS,
                right: cx + CURSOR_RADIUS,
                bottom: cy + CURSOR_RADIUS,
            })?;
        }

        hdc.BitBlt(
            POINT { x: 0, y: 0 },
            SIZE { cx: w_, cy: h_ },
            &mem,
            POINT { x: 0, y: 0 },
            co::ROP::SRCCOPY,
        )?;
        Ok(())
    }

    // ── 기록 라벨 갱신 ──
    fn update_stats(&self) {
        let st = self.state.borrow();
        let s = &st.stats;
        let avg_react = if s.hits > 0 {
            s.react_sum / s.hits as f64
        } else {
            0.0
        };
        let avg_acc = if s.hits > 0 {
            s.acc_sum / s.hits as f64 * 100.0
        } else {
            0.0
        };
        let total = s.hits + s.misclicks;
        let click_acc = if total > 0 {
            s.hits as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let _ = self.l_score.hwnd().SetWindowText(&format!("{:.0}", s.score));
        let _ = self.l_hits.hwnd().SetWindowText(&format!(
            "{}  ({} · {})",
            s.hits, s.expired, s.misclicks
        ));
        let _ = self.l_acc.hwnd().SetWindowText(&format!("{:.1}%", click_acc));
        let _ = self
            .l_react
            .hwnd()
            .SetWindowText(&format!("{:.0} ms", avg_react));
        let _ = self
            .l_best
            .hwnd()
            .SetWindowText(&format!("{:.0} ms", s.react_best));
        let _ = self
            .l_center
            .hwnd()
            .SetWindowText(&format!("{:.1}%", avg_acc));
    }

    fn set_status(&self, text: &str) {
        let _ = self.l_status.hwnd().SetWindowText(text);
    }
}

fn parse_f64(e: &gui::Edit) -> Option<f64> {
    e.text().ok().and_then(|s| s.trim().parse::<f64>().ok())
}

// DPI/감도는 라벨 표시와 실제 시뮬레이션이 동일한 값을 쓰도록 한 곳에서 결정.
fn resolve_dpi(e: &gui::Edit) -> f64 {
    parse_f64(e).unwrap_or(800.0).clamp(100.0, 32000.0)
}

fn resolve_sens(e: &gui::Edit) -> f64 {
    parse_f64(e).unwrap_or(2.0).clamp(0.01, 100.0)
}

fn resolve_yaw(e: &gui::Edit) -> f64 {
    parse_f64(e).unwrap_or(0.022).clamp(0.0001, 10.0)
}

fn main() {
    let app = App::new();
    if let Err(e) = app.run() {
        eprintln!("error: {}", e);
    }
}
