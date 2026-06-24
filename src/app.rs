//! GUI 구성 · 이벤트 배선 · 세션(시작/정지) · raw input · vsync 프레임 오케스트레이션.
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winsafe::{self as w, co, gui, prelude::*};
use winsafe::{POINT, RECT, SIZE};

use crate::model::*;
use crate::patterns::step_target;
use crate::persist::{append_csv, save_best};
use crate::raw;
use crate::render;
use crate::theme::*;

// 감도: pixels_per_count = sens × yaw × px_per_deg × speed. px_per_deg 는 모니터 전체 기준.
const VFOV_DEG: f64 = 103.0; // 게임 수직 시야각 (오버워치 기본 103)
const DEG2RAD: f64 = std::f64::consts::PI / 180.0;
const FALLBACK_FRAME: Duration = Duration::from_micros(6_944); // DwmFlush 실패 시 ~144Hz

#[derive(Clone)]
pub struct App {
    wnd: gui::WindowMain,
    aim: gui::WindowControl,

    btn_mode: gui::Button,

    e_dpi: gui::Edit,
    e_sens: gui::Edit,
    e_yaw: gui::Edit,
    e_speed: gui::Edit,
    e_radius: gui::Edit,
    e_track: gui::Edit,
    e_spawn: gui::Edit,
    e_life: gui::Edit,
    e_max: gui::Edit,

    l_edpi: gui::Label,
    l_cm360: gui::Label,
    btn_start: gui::Button,
    btn_reset: gui::Button,
    btn_heat: gui::Button,

    l_score: gui::Label,
    l_bestscore: gui::Label,
    l_acc: gui::Label,
    l_hits: gui::Label,
    l_react: gui::Label,
    l_react_best: gui::Label,
    l_center: gui::Label,
    l_status: gui::Label,

    state: Rc<RefCell<State>>,
    session_active: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    frame_pending: Arc<AtomicBool>,
}

impl App {
    pub fn new() -> Self {
        let panel_brush = leak_brush(C_PANEL);
        let editor_brush = leak_brush(C_EDITOR);

        let wnd = gui::WindowMain::new(gui::WindowMainOpts {
            title: "Notes",
            size: (1024, 800),
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
                size: (694, 730),
                class_bg_brush: gui::Brush::Handle(editor_brush),
                class_cursor: gui::Cursor::Idc(co::IDC::ARROW),
                ..Default::default()
            },
        );

        const LX: i32 = 716;
        const VX: i32 = 904;
        const VW: i32 = 100;
        let lbl = |p: &gui::WindowMain, text: &str, x: i32, y: i32| {
            gui::Label::new(p, gui::LabelOpts { text: &text.to_owned(), position: (x, y), ..Default::default() })
        };
        let val = |p: &gui::WindowMain, text: &str, y: i32| {
            gui::Label::new(
                p,
                gui::LabelOpts { text: &text.to_owned(), position: (VX, y), size: (188, 18), ..Default::default() },
            )
        };
        let edit = |p: &gui::WindowMain, text: &str, y: i32| {
            gui::Edit::new(
                p,
                gui::EditOpts { text: &text.to_owned(), position: (VX, y - 2), width: VW, height: 22, ..Default::default() },
            )
        };

        let _hdr1 = lbl(&wnd, "설정", LX, 10);
        let btn_mode = gui::Button::new(
            &wnd,
            gui::ButtonOpts { text: SCENARIOS[0].name, position: (LX, 32), width: 284, height: 26, ..Default::default() },
        );

        let _l1 = lbl(&wnd, "마우스 DPI", LX, 66);
        let e_dpi = edit(&wnd, "800", 66);
        let _l2 = lbl(&wnd, "감도 (오버워치 인게임)", LX, 92);
        let e_sens = edit(&wnd, "5.0", 92);
        let _l3 = lbl(&wnd, "게임 yaw (°/카운트)", LX, 118);
        let e_yaw = edit(&wnd, "0.0066", 118);
        let _l4 = lbl(&wnd, "eDPI (DPI × 감도)", LX, 144);
        let l_edpi = val(&wnd, "4000", 144);
        let _l5 = lbl(&wnd, "cm/360 (≈게임 체감)", LX, 168);
        let l_cm360 = val(&wnd, "—", 168);
        let _l6 = lbl(&wnd, "커서 속도 배율", LX, 194);
        let e_speed = edit(&wnd, "1.0", 194);
        let _l7 = lbl(&wnd, "타깃 크기 (반지름 px)", LX, 220);
        let e_radius = edit(&wnd, "28", 220);
        let _l8 = lbl(&wnd, "트래킹 타깃 속도 (px/s)", LX, 246);
        let e_track = edit(&wnd, "700", 246);
        let _l9 = lbl(&wnd, "생성 주기 (ms, 클릭)", LX, 272);
        let e_spawn = edit(&wnd, "850", 272);
        let _l10 = lbl(&wnd, "타깃 수명 (ms, 클릭)", LX, 298);
        let e_life = edit(&wnd, "1100", 298);
        let _l11 = lbl(&wnd, "최대 동시 타깃 (클릭)", LX, 324);
        let e_max = edit(&wnd, "4", 324);

        let btn_start = gui::Button::new(
            &wnd,
            gui::ButtonOpts { text: "시작 (F2)", position: (LX, 356), width: 170, height: 30, ..Default::default() },
        );
        let btn_reset = gui::Button::new(
            &wnd,
            gui::ButtonOpts { text: "기록 초기화", position: (LX + 184, 356), width: 100, height: 30, ..Default::default() },
        );
        let btn_heat = gui::Button::new(
            &wnd,
            gui::ButtonOpts { text: "히트맵: 꺼짐", position: (LX, 392), width: 170, height: 26, ..Default::default() },
        );

        let _hdr2 = lbl(&wnd, "기록", LX, 426);
        let _s1 = lbl(&wnd, "점수", LX, 450);
        let l_score = val(&wnd, "0", 450);
        let _s2 = lbl(&wnd, "최고 점수", LX, 474);
        let l_bestscore = val(&wnd, "0", 474);
        let _s3 = lbl(&wnd, "정확도", LX, 498);
        let l_acc = val(&wnd, "0.0%", 498);
        let _s4 = lbl(&wnd, "명중 (놓침·빗맞힘)", LX, 522);
        let l_hits = val(&wnd, "0", 522);
        let _s5 = lbl(&wnd, "평균 반응속도", LX, 546);
        let l_react = val(&wnd, "0 ms", 546);
        let _s6 = lbl(&wnd, "최고 반응속도", LX, 570);
        let l_react_best = val(&wnd, "0 ms", 570);
        let _s7 = lbl(&wnd, "평균 중앙 정확도", LX, 594);
        let l_center = val(&wnd, "0.0%", 594);

        let l_status = gui::Label::new(
            &wnd,
            gui::LabelOpts { text: "정지됨. 모드 선택 후 [시작] 또는 F2.", position: (LX, 626), size: (300, 30), ..Default::default() },
        );
        let _hint = gui::Label::new(
            &wnd,
            gui::LabelOpts {
                text: "클릭: 타깃 좌클릭. 트래킹: 좌클릭을 누른 채 초록 링을 타깃에 유지. \
                       ESC 정지. 기록은 aim_log.csv 에 자동 저장. 가속(포인터 정확도 향상)은 꺼두세요.",
                position: (LX, 660),
                size: (300, 96),
                ..Default::default()
            },
        );

        Self {
            wnd,
            aim,
            btn_mode,
            e_dpi,
            e_sens,
            e_yaw,
            e_speed,
            e_radius,
            e_track,
            e_spawn,
            e_life,
            e_max,
            l_edpi,
            l_cm360,
            btn_start,
            btn_reset,
            btn_heat,
            l_score,
            l_bestscore,
            l_acc,
            l_hits,
            l_react,
            l_react_best,
            l_center,
            l_status,
            state: Rc::new(RefCell::new(State::new())),
            session_active: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(true)),
            frame_pending: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(&self) -> w::AnyResult<i32> {
        self.events();
        self.wnd.run_main(None)
    }

    fn events(&self) {
        let panel_brush = leak_brush(C_PANEL);
        let edit_brush = leak_brush(C_EDIT_BG);

        self.wnd.on().wm_ctl_color_static(move |p| {
            p.hdc.SetTextColor(rgb(C_TEXT))?;
            p.hdc.SetBkColor(rgb(C_PANEL))?;
            Ok(unsafe { panel_brush.raw_copy() })
        });
        self.wnd.on().wm_ctl_color_edit(move |p| {
            p.hdc.SetTextColor(rgb(C_TEXT))?;
            p.hdc.SetBkColor(rgb(C_EDIT_BG))?;
            Ok(unsafe { edit_brush.raw_copy() })
        });

        // 생성: raw input 등록 + vsync 페이서 스레드
        let me = self.clone();
        self.wnd.on().wm_create(move |_| {
            let hwnd_ptr = me.wnd.hwnd().ptr();
            let dev = raw::RawInputDevice {
                us_usage_page: raw::HID_USAGE_PAGE_GENERIC,
                us_usage: raw::HID_USAGE_GENERIC_MOUSE,
                dw_flags: 0,
                hwnd_target: hwnd_ptr,
            };
            let ok = unsafe {
                raw::RegisterRawInputDevices(&dev, 1, std::mem::size_of::<raw::RawInputDevice>() as u32)
            } != 0;
            me.state.borrow_mut().raw_input_ok = ok;
            if !ok {
                me.set_status("Raw input 등록 실패 — 마우스 추적 불가. 앱을 재시작해 보세요.");
            }
            me.update_edpi();
            me.update_stats();

            let hwnd_usize = hwnd_ptr as usize;
            let alive = me.alive.clone();
            let active = me.session_active.clone();
            let pending = me.frame_pending.clone();
            std::thread::spawn(move || {
                while alive.load(Ordering::Relaxed) {
                    let hr = unsafe { raw::DwmFlush() };
                    if hr != 0 {
                        std::thread::sleep(FALLBACK_FRAME);
                    }
                    if !alive.load(Ordering::Relaxed) {
                        break;
                    }
                    if active.load(Ordering::Relaxed) && !pending.swap(true, Ordering::AcqRel) {
                        unsafe {
                            raw::PostMessageW(hwnd_usize as *mut c_void, raw::WM_APP_FRAME, 0, 0);
                        }
                    }
                }
            });
            Ok(0)
        });

        // 모드 순환 버튼
        let me = self.clone();
        self.btn_mode.on().bn_clicked(move || {
            {
                let mut st = me.state.borrow_mut();
                if st.running {
                    return Ok(());
                }
                st.scenario = (st.scenario + 1) % SCENARIOS.len();
            }
            let name = me.state.borrow().scn().name;
            let _ = me.btn_mode.hwnd().SetWindowText(name);
            me.update_stats();
            Ok(())
        });

        // eDPI/cm360 실시간 갱신
        for e in [&self.e_dpi, &self.e_sens, &self.e_yaw] {
            let me = self.clone();
            e.on().en_change(move || {
                me.update_edpi();
                Ok(())
            });
        }

        let me = self.clone();
        self.btn_start.on().bn_clicked(move || {
            me.toggle();
            Ok(())
        });
        let me = self.clone();
        self.btn_reset.on().bn_clicked(move || {
            {
                let mut st = me.state.borrow_mut();
                st.stats = Stats::default();
                st.clicks.clear();
            }
            me.update_stats();
            let _ = me.aim.hwnd().InvalidateRect(None, true);
            Ok(())
        });
        let me = self.clone();
        self.btn_heat.on().bn_clicked(move || {
            let on = {
                let mut st = me.state.borrow_mut();
                st.show_heatmap = !st.show_heatmap;
                st.show_heatmap
            };
            let _ = me.btn_heat.hwnd().SetWindowText(if on { "히트맵: 켜짐" } else { "히트맵: 꺼짐" });
            let _ = me.aim.hwnd().InvalidateRect(None, true);
            Ok(())
        });

        // Raw input(이동) → 가상 커서 누적
        let me = self.clone();
        self.wnd.on().wm(co::WM::INPUT, move |p| {
            me.on_raw_input(p.lparam);
            let res = unsafe { me.wnd.hwnd().DefWindowProc(p) };
            Ok(res)
        });

        // vsync 프레임
        let me = self.clone();
        let wm_frame = unsafe { co::WM::from_raw(raw::WM_APP_FRAME) };
        self.wnd.on().wm(wm_frame, move |_| {
            me.frame_pending.store(false, Ordering::Release);
            me.frame();
            Ok(0)
        });

        let me = self.clone();
        self.wnd.on().wm_destroy(move || {
            me.alive.store(false, Ordering::Release);
            Ok(())
        });

        // 페인팅
        let me = self.clone();
        self.aim.on().wm_paint(move || {
            me.on_paint()?;
            Ok(())
        });
        self.aim.on().wm_erase_bkgnd(|_| Ok(1));

        // 좌클릭 다운/업
        let me = self.clone();
        self.aim.on().wm_l_button_down(move |_| {
            me.on_mouse_down();
            Ok(())
        });
        let me = self.clone();
        self.aim.on().wm_l_button_up(move |_| {
            me.state.borrow_mut().firing = false;
            Ok(())
        });

        // 키: ESC 정지 / F2 토글
        let me = self.clone();
        self.aim.on().wm_key_down(move |k| {
            if k.vkey_code == co::VK::ESCAPE {
                me.stop();
            } else if k.vkey_code == co::VK::F2 {
                me.toggle();
            }
            Ok(())
        });

        // 포커스 상실 시 정지(시작/정지 버튼으로 가는 경우 제외)
        let me = self.clone();
        self.aim.on().wm_kill_focus(move |p| {
            let to_button = p.hwnd.as_ref().map(|h| h.ptr() == me.btn_start.hwnd().ptr()).unwrap_or(false);
            if !to_button {
                me.stop();
            }
            Ok(())
        });
    }

    fn update_edpi(&self) {
        let dpi = resolve_dpi(&self.e_dpi);
        let sens = resolve_sens(&self.e_sens);
        let yaw = resolve_yaw(&self.e_yaw);
        let _ = self.l_edpi.hwnd().SetWindowText(&format!("{:.0}", dpi * sens));
        let denom = dpi * sens * yaw;
        let cm360 = if denom > 0.0 { 360.0 * 2.54 / denom } else { 0.0 };
        let _ = self.l_cm360.hwnd().SetWindowText(&format!("{:.1} cm", cm360));
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
                self.set_status("Raw input 사용 불가 — 시작할 수 없습니다.");
                return;
            }
        }
        let rc = self.aim.hwnd().GetClientRect().unwrap_or(RECT { left: 0, top: 0, right: 1, bottom: 1 });

        let settings = Settings {
            sens: resolve_sens(&self.e_sens),
            yaw: resolve_yaw(&self.e_yaw),
            speed: parse_f64(&self.e_speed).unwrap_or(1.0).clamp(0.05, 20.0),
            radius: parse_f64(&self.e_radius).unwrap_or(28.0).clamp(6.0, 160.0) as i32,
            track_speed: parse_f64(&self.e_track).unwrap_or(700.0).clamp(50.0, 6000.0),
            spawn_ms: parse_f64(&self.e_spawn).unwrap_or(850.0).clamp(50.0, 10000.0) as u64,
            life_ms: parse_f64(&self.e_life).unwrap_or(1100.0).clamp(100.0, 20000.0) as u64,
            max_targets: parse_f64(&self.e_max).unwrap_or(4.0).clamp(1.0, 50.0) as usize,
        };

        // 모니터 픽셀/° (게임 손맛 기준)
        let sw = unsafe { raw::GetSystemMetrics(raw::SM_CXSCREEN) }.max(1) as f64;
        let sh = unsafe { raw::GetSystemMetrics(raw::SM_CYSCREEN) }.max(1) as f64;
        let hfov_deg = 2.0 * ((VFOV_DEG * 0.5 * DEG2RAD).tan() * (sw / sh)).atan() / DEG2RAD;
        let px_per_deg = sw / hfov_deg;

        let now = Instant::now();
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
            st.last_frame = None;
            st.session_start = Some(now);
            st.firing = false;
            st.stats = Stats::default();
            st.running = true;
        }
        self.session_active.store(true, Ordering::Release);

        w::ShowCursor(false);
        if let Ok(screen) = self.aim.hwnd().ClientToScreenRc(rc) {
            let _ = w::ClipCursor(Some(&screen));
        }
        if let Ok(center) = self.aim.hwnd().ClientToScreen(POINT { x: rc.right / 2, y: rc.bottom / 2 }) {
            let _ = w::SetCursorPos(center.x, center.y);
        }
        let _ = self.aim.hwnd().SetFocus();
        let _ = self.btn_start.hwnd().SetWindowText("정지 (ESC)");
        let scn = self.state.borrow().scn();
        self.set_status(match scn.mode {
            Mode::Click => "진행 중 — 타깃 좌클릭. ESC로 정지.",
            Mode::Track => "진행 중 — 좌클릭 누른 채 초록 링을 타깃에 유지. ESC로 정지.",
        });
        self.update_stats();
    }

    fn stop(&self) {
        if !self.state.borrow().running {
            return;
        }
        // 라운드 요약 → 기록/CSV/최고기록
        let summary = {
            let mut st = self.state.borrow_mut();
            st.running = false;
            st.firing = false;
            let scn = st.scn();
            let dur = st.session_start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
            let rs = round_score(&st, scn.mode);
            let acc = accuracy_pct(&st, scn.mode);
            // 2초 이상 라운드만 기록(최고기록/히스토리/CSV 일관)
            if dur >= 2.0 {
                if rs > st.best {
                    st.best = rs;
                    save_best(st.best);
                }
                st.history.push(rs);
                if st.history.len() > MAX_HISTORY {
                    let d = st.history.len() - MAX_HISTORY;
                    st.history.drain(0..d);
                }
            }
            st.targets.clear();
            (scn.name, dur, rs, acc)
        };
        self.session_active.store(false, Ordering::Release);
        let _ = w::ClipCursor(None);
        w::ShowCursor(true);
        let _ = self.btn_start.hwnd().SetWindowText("시작 (F2)");

        if summary.1 >= 2.0 {
            append_csv(summary.0, summary.1, summary.2, summary.3);
            self.set_status(&format!(
                "정지됨 · 점수 {:.0} · 정확도 {:.1}% · {:.0}s (aim_log.csv 저장)",
                summary.2, summary.3, summary.1
            ));
        } else {
            self.set_status("정지됨. [시작] 또는 F2.");
        }
        self.update_stats();
        let _ = self.aim.hwnd().InvalidateRect(None, true);
    }

    // ── Raw input → 가상 커서 누적 ──
    fn on_raw_input(&self, lparam: isize) {
        let hri = lparam as *mut c_void;
        let hdr = std::mem::size_of::<raw::RawInputHeader>() as u32;
        let mut ri_buf = std::mem::MaybeUninit::<raw::RawInput>::uninit();
        let mut size = std::mem::size_of::<raw::RawInput>() as u32;
        let got = unsafe {
            raw::GetRawInputData(hri, raw::RID_INPUT, ri_buf.as_mut_ptr() as *mut c_void, &mut size, hdr)
        };
        if got == u32::MAX || got == 0 {
            return;
        }
        let ri = unsafe { &*ri_buf.as_ptr() };
        if ri.header.dw_type != raw::RIM_TYPEMOUSE || ri.mouse.us_flags & raw::MOUSE_MOVE_ABSOLUTE != 0 {
            return;
        }
        let dx = ri.mouse.l_last_x as f64;
        let dy = ri.mouse.l_last_y as f64;

        let mut st = self.state.borrow_mut();
        if !st.running {
            return;
        }
        if dx != 0.0 || dy != 0.0 {
            let ppc = st.settings.sens * st.settings.yaw * st.px_per_deg * st.settings.speed;
            let (w_, h_) = (st.aim_w as f64, st.aim_h as f64);
            st.cx = (st.cx + dx * ppc).clamp(0.0, w_);
            st.cy = (st.cy + dy * ppc).clamp(0.0, h_);
        }
    }

    // ── 좌클릭 다운: 발사 시작 + (클릭 모드) 명중 판정 ──
    fn on_mouse_down(&self) {
        let mode;
        {
            let mut st = self.state.borrow_mut();
            if !st.running {
                return;
            }
            st.firing = true;
            mode = st.scn().mode;
            if mode == Mode::Click {
                let (cx, cy) = (st.cx, st.cy);
                let mut hit: Option<usize> = None;
                let mut best_d = f64::MAX;
                for (i, t) in st.targets.iter().enumerate() {
                    let d = ((t.x - cx).powi(2) + (t.y - cy).powi(2)).sqrt();
                    if d <= t.r as f64 && d < best_d {
                        best_d = d;
                        hit = Some(i);
                    }
                }
                let mark_hit = hit.is_some();
                if let Some(i) = hit {
                    let t = st.targets.remove(i);
                    let react = t.born.elapsed().as_secs_f64() * 1000.0;
                    let acc = (1.0 - best_d / t.r as f64).clamp(0.0, 1.0);
                    let life = st.settings.life_ms as f64;
                    let sf = (1.0 - react / life).clamp(0.0, 1.0);
                    let pts = 100.0 * (0.25 + 0.75 * acc) * (0.35 + 0.65 * sf);
                    st.stats.score += pts;
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
                let (mx, my) = (cx as i32, cy as i32);
                st.clicks.push(ClickMark { x: mx, y: my, hit: mark_hit });
                if st.clicks.len() > MAX_CLICK_MARKS {
                    let d = st.clicks.len() - MAX_CLICK_MARKS;
                    st.clicks.drain(0..d);
                }
            }
        }
        if mode == Mode::Click {
            self.update_stats();
            let _ = self.aim.hwnd().InvalidateRect(None, false);
            let _ = self.aim.hwnd().UpdateWindow();
        }
    }

    // ── 한 프레임: 전진 + 렌더 (vsync 페이서가 호출) ──
    fn frame(&self) {
        let mut do_stats = false;
        {
            let mut st = self.state.borrow_mut();
            if !st.running {
                return;
            }
            let now = Instant::now();
            let dt = match st.last_frame {
                Some(p) => (now - p).as_secs_f64().min(0.1),
                None => 0.0,
            };
            st.last_frame = Some(now);

            let scn = st.scn();
            let (w_, h_) = (st.aim_w as f64, st.aim_h as f64);

            match scn.mode {
                Mode::Click => {
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
                        let m = (r + 4) as f64;
                        let x = rng_range(&mut st.rng, m, (w_ - m).max(m + 1.0));
                        let y = rng_range(&mut st.rng, m, (h_ - m).max(m + 1.0));
                        st.targets.push(Target { x, y, r, born: now, vx: 0.0, vy: 0.0, phase: 0.0, t: 0.0, base_y: y });
                        st.last_spawn = Some(now);
                    }
                }
                Mode::Track => {
                    // 단일 타깃 유지
                    if st.targets.is_empty() {
                        let r = st.settings.radius;
                        let by = h_ * 0.5;
                        let sp = st.settings.track_speed;
                        st.targets.push(Target {
                            x: w_ * 0.5,
                            y: by,
                            r,
                            born: now,
                            vx: sp,
                            vy: 0.0,
                            phase: 0.0,
                            t: 0.0,
                            base_y: if scn.pattern == Pattern::Arc { h_ * 0.4 } else { by },
                        });
                    }
                    // 이동 (split-borrow로 targets와 rng를 동시에 &mut)
                    let sp = st.settings.track_speed;
                    let State { targets, rng, .. } = &mut *st;
                    step_target(&mut targets[0], scn.pattern, dt, w_, h_, sp, rng);

                    // 추적 점수 (발사 중 & 링 안일 때)
                    if st.firing && dt > 0.0 {
                        st.stats.track_total += dt;
                        let t0 = &st.targets[0];
                        let d = ((t0.x - st.cx).powi(2) + (t0.y - st.cy).powi(2)).sqrt();
                        if d <= t0.r as f64 {
                            st.stats.track_on += dt;
                            st.stats.score += dt * 100.0;
                        }
                        do_stats = true;
                    }
                }
            }
        }
        // 동기 렌더 (큐를 거치지 않으므로 WM_INPUT 폭주에도 굶지 않음)
        let _ = self.aim.hwnd().InvalidateRect(None, false);
        let _ = self.aim.hwnd().UpdateWindow();
        if do_stats {
            self.update_stats();
        }
    }

    // ── 페인팅 (이중 버퍼 설정 + render::draw_world + BitBlt) ──
    fn on_paint(&self) -> w::AnyResult<()> {
        let hdc = self.aim.hwnd().BeginPaint()?;
        let rc = self.aim.hwnd().GetClientRect()?;
        let mem = hdc.CreateCompatibleDC()?;
        let bmp = hdc.CreateCompatibleBitmap(rc.right, rc.bottom)?;
        let _old_bmp = mem.SelectObject(&*bmp)?;
        {
            let st = self.state.borrow();
            render::draw_world(&mem, rc, &st)?;
        }
        hdc.BitBlt(POINT { x: 0, y: 0 }, SIZE { cx: rc.right, cy: rc.bottom }, &mem, POINT { x: 0, y: 0 }, co::ROP::SRCCOPY)?;
        Ok(())
    }

    // ── 기록 라벨 (모드별) ──
    fn update_stats(&self) {
        let st = self.state.borrow();
        let s = &st.stats;
        let mode = st.scn().mode;
        let _ = self.l_bestscore.hwnd().SetWindowText(&format!("{:.0}", st.best));
        match mode {
            Mode::Click => {
                let avg_r = if s.hits > 0 { s.react_sum / s.hits as f64 } else { 0.0 };
                let avg_c = if s.hits > 0 { s.acc_sum / s.hits as f64 * 100.0 } else { 0.0 };
                let _ = self.l_score.hwnd().SetWindowText(&format!("{:.0}", s.score));
                let _ = self.l_acc.hwnd().SetWindowText(&format!("{:.1}%", accuracy_pct(&st, mode)));
                let _ = self.l_hits.hwnd().SetWindowText(&format!("{}  ({} · {})", s.hits, s.expired, s.misclicks));
                let _ = self.l_react.hwnd().SetWindowText(&format!("{:.0} ms", avg_r));
                let _ = self.l_react_best.hwnd().SetWindowText(&format!("{:.0} ms", s.react_best));
                let _ = self.l_center.hwnd().SetWindowText(&format!("{:.1}%", avg_c));
            }
            Mode::Track => {
                let _ = self.l_score.hwnd().SetWindowText(&format!("{:.0}", s.score));
                let _ = self.l_acc.hwnd().SetWindowText(&format!("{:.1}%", accuracy_pct(&st, mode)));
                let _ = self.l_hits.hwnd().SetWindowText(&format!("{:.1}s 추적", s.track_on));
                let _ = self.l_react.hwnd().SetWindowText("—");
                let _ = self.l_react_best.hwnd().SetWindowText("—");
                let _ = self.l_center.hwnd().SetWindowText("—");
            }
        }
    }

    fn set_status(&self, text: &str) {
        let _ = self.l_status.hwnd().SetWindowText(text);
    }
}

// ── 입력 파싱/클램프 (라벨 표시와 시뮬레이션이 동일 값을 쓰도록) ──
fn parse_f64(e: &gui::Edit) -> Option<f64> {
    e.text().ok().and_then(|s| s.trim().parse::<f64>().ok())
}
fn resolve_dpi(e: &gui::Edit) -> f64 {
    parse_f64(e).unwrap_or(800.0).clamp(100.0, 32000.0)
}
fn resolve_sens(e: &gui::Edit) -> f64 {
    parse_f64(e).unwrap_or(5.0).clamp(0.01, 100.0)
}
fn resolve_yaw(e: &gui::Edit) -> f64 {
    parse_f64(e).unwrap_or(0.0066).clamp(0.0001, 10.0)
}
