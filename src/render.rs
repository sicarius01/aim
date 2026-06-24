//! 에임 영역 장면 렌더링. 전달받은 (이중 버퍼) 메모리 DC에 한 프레임을 그린다.
use winsafe::{self as w, co};
use winsafe::{HBRUSH, HDC, HPEN, RECT};

use crate::model::{accuracy_pct, Mode, State};
use crate::theme::*;

/// 배경 · 타깃 · 히트맵 · 진행 스파크라인 · 커서 링 · 라이브 텍스트를 mem DC에 그린다.
/// (BeginPaint/CreateCompatibleDC/BitBlt 는 호출부가 담당)
pub fn draw_world(mem: &HDC, rc: RECT, st: &State) -> w::AnyResult<()> {
    let h_ = rc.bottom;

    // 배경
    let bg = HBRUSH::CreateSolidBrush(rgb(C_EDITOR))?;
    mem.FillRect(rc, &bg)?;

    let mode = st.scn().mode;
    let cursor_over = st.running
        && st
            .targets
            .iter()
            .any(|t| ((t.x - st.cx).powi(2) + (t.y - st.cy).powi(2)).sqrt() <= t.r as f64);

    // 타깃들 (트래킹 중 링 안이면 hot 색)
    {
        let fill = if mode == Mode::Track && cursor_over { C_TARGET_HOT } else { C_TARGET };
        let tb = HBRUSH::CreateSolidBrush(rgb(fill))?;
        let tp = HPEN::CreatePen(co::PS::SOLID, 1, rgb(C_TARGET_EDGE))?;
        let _ob = mem.SelectObject(&*tb)?;
        let _op = mem.SelectObject(&*tp)?;
        for t in &st.targets {
            let (x, y, r) = (t.x as i32, t.y as i32, t.r);
            mem.Ellipse(RECT { left: x - r, top: y - r, right: x + r, bottom: y + r })?;
        }
    }

    // 히트맵 (클릭 위치 점: 명중 초록 / 빗맞힘 빨강)
    if st.show_heatmap && !st.clicks.is_empty() {
        for m in &st.clicks {
            let col = if m.hit { C_HIT } else { C_MISS };
            let hb = HBRUSH::CreateSolidBrush(rgb(col))?;
            let hp = HPEN::CreatePen(co::PS::SOLID, 1, rgb(col))?;
            let _ob = mem.SelectObject(&*hb)?;
            let _op = mem.SelectObject(&*hp)?;
            mem.Ellipse(RECT { left: m.x - 3, top: m.y - 3, right: m.x + 3, bottom: m.y + 3 })?;
        }
    }

    // 진행 스파크라인 (좌상단, 최근 라운드 점수)
    if st.history.len() >= 2 {
        let (gx, gy, gw, gh) = (12, 12, 150, 40);
        let mn = st.history.iter().cloned().fold(f64::MAX, f64::min);
        let mx = st.history.iter().cloned().fold(f64::MIN, f64::max);
        let span = (mx - mn).max(1.0);
        let sp = HPEN::CreatePen(co::PS::SOLID, 1, rgb(C_SPARK))?;
        let _op = mem.SelectObject(&*sp)?;
        let n = st.history.len();
        for i in 0..n {
            let px = gx + (gw * i as i32) / (n as i32 - 1);
            let py = gy + gh - ((st.history[i] - mn) / span * gh as f64) as i32;
            if i == 0 {
                mem.MoveToEx(px, py, None)?;
            } else {
                mem.LineTo(px, py)?;
            }
        }
    }

    // 커서 링 (진행 중, 빈 초록 원)
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

    // 라이브 텍스트 (트래킹 정확도 / 클릭 명중·정확도)
    if st.running {
        mem.SetBkMode(co::BKMODE::TRANSPARENT)?;
        mem.SetTextColor(rgb(C_TEXT))?;
        let txt = match mode {
            Mode::Track => format!("추적 {:.0}%", accuracy_pct(st, Mode::Track)),
            Mode::Click => format!("명중 {}  정확도 {:.0}%", st.stats.hits, accuracy_pct(st, Mode::Click)),
        };
        mem.TextOut(12, h_ - 24, &txt)?;
    }

    Ok(())
}
