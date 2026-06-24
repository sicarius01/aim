//! 위젯 생성 + 정적 패널 레이아웃. 동작 로직은 없다(app.rs 담당).
use winsafe::{co, gui};

use crate::model::SCENARIOS;
use crate::theme::{leak_brush, C_EDITOR, C_PANEL};

/// 메인 창과 모든 컨트롤. app::App 이 이걸 받아 상태/원자 플래그와 함께 조립한다.
pub struct Widgets {
    pub wnd: gui::WindowMain,
    pub aim: gui::WindowControl,

    pub btn_mode: gui::Button,

    pub e_dpi: gui::Edit,
    pub e_sens: gui::Edit,
    pub e_yaw: gui::Edit,
    pub e_speed: gui::Edit,
    pub e_radius: gui::Edit,
    pub e_track: gui::Edit,
    pub e_spawn: gui::Edit,
    pub e_life: gui::Edit,
    pub e_max: gui::Edit,

    pub l_edpi: gui::Label,
    pub l_cm360: gui::Label,
    pub btn_start: gui::Button,
    pub btn_reset: gui::Button,
    pub btn_heat: gui::Button,

    pub l_score: gui::Label,
    pub l_bestscore: gui::Label,
    pub l_acc: gui::Label,
    pub l_hits: gui::Label,
    pub l_react: gui::Label,
    pub l_react_best: gui::Label,
    pub l_center: gui::Label,
    pub l_status: gui::Label,
}

/// 창 + 컨트롤을 생성하고 배치한다.
pub fn build() -> Widgets {
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

    Widgets {
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
    }
}
