//! 도메인 모델: 모드/시나리오/설정/타깃/상태 + RNG + 점수 헬퍼.
use std::time::Instant;

use crate::persist::load_best;

pub const MAX_CLICK_MARKS: usize = 160;
pub const MAX_HISTORY: usize = 40;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Click,
    Track,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Pattern {
    Static,
    Strafe,
    Air,
    Blink, // 트레이서: 주기적 순간이동
    Arc,   // 파라: 느린 큰 아크
    Dash,  // 겐지: 정지 후 빠른 직선 버스트
}

#[derive(Clone, Copy)]
pub struct Scenario {
    pub mode: Mode,
    pub pattern: Pattern,
    pub name: &'static str,
}

pub const SCENARIOS: [Scenario; 6] = [
    Scenario { mode: Mode::Click, pattern: Pattern::Static, name: "클릭 · 정적 다중" },
    Scenario { mode: Mode::Track, pattern: Pattern::Strafe, name: "트래킹 · 스트레이프" },
    Scenario { mode: Mode::Track, pattern: Pattern::Air, name: "트래킹 · 에어" },
    Scenario { mode: Mode::Track, pattern: Pattern::Blink, name: "트래킹 · 트레이서(블링크)" },
    Scenario { mode: Mode::Track, pattern: Pattern::Arc, name: "트래킹 · 파라(아크)" },
    Scenario { mode: Mode::Track, pattern: Pattern::Dash, name: "트래킹 · 겐지(대시)" },
];

// DPI는 시뮬레이션(ppc)에 직접 쓰이지 않고 eDPI/cm360 표시에만 쓴다.
#[derive(Clone, Copy)]
pub struct Settings {
    pub sens: f64,
    pub yaw: f64,
    pub speed: f64,
    pub radius: i32,
    pub track_speed: f64, // 트래킹 타깃 기본 속도 (px/s)
    pub spawn_ms: u64,
    pub life_ms: u64,
    pub max_targets: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sens: 5.0,   // 오버워치 기본(4~6 중간)
            yaw: 0.0066, // 오버워치 yaw
            speed: 1.0,
            radius: 28,
            track_speed: 700.0,
            spawn_ms: 850,
            life_ms: 1100,
            max_targets: 4,
        }
    }
}

pub struct Target {
    pub x: f64,
    pub y: f64,
    pub r: i32,
    pub born: Instant,
    pub vx: f64,
    pub vy: f64,
    pub phase: f64, // 패턴 이벤트까지 남은 시간(초)
    pub t: f64,     // 파라메트릭 경로용 누적 시간
    pub base_y: f64,
}

#[derive(Default)]
pub struct Stats {
    pub score: f64,
    pub hits: u32,
    pub misclicks: u32,
    pub expired: u32,
    pub react_sum: f64,
    pub react_best: f64,
    pub acc_sum: f64,
    pub track_on: f64,    // 발사 중 타깃 위에 머문 시간(초)
    pub track_total: f64, // 발사한 총 시간(초)
}

pub struct ClickMark {
    pub x: i32,
    pub y: i32,
    pub hit: bool,
}

pub struct State {
    pub settings: Settings,
    pub scenario: usize,
    pub running: bool,
    pub raw_input_ok: bool,
    pub firing: bool, // 좌클릭 누름 상태(트래킹 발사)
    pub targets: Vec<Target>,
    pub cx: f64,
    pub cy: f64,
    pub aim_w: i32,
    pub aim_h: i32,
    pub px_per_deg: f64,
    pub last_spawn: Option<Instant>,
    pub last_frame: Option<Instant>,
    pub session_start: Option<Instant>,
    pub rng: u64,
    pub stats: Stats,
    pub clicks: Vec<ClickMark>,
    pub show_heatmap: bool,
    pub best: f64,
    pub history: Vec<f64>,
}

impl State {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
        Self {
            settings: Settings::default(),
            scenario: 0,
            running: false,
            raw_input_ok: false,
            firing: false,
            targets: Vec::new(),
            cx: 0.0,
            cy: 0.0,
            aim_w: 1,
            aim_h: 1,
            px_per_deg: 1.0,
            last_spawn: None,
            last_frame: None,
            session_start: None,
            rng: seed,
            stats: Stats::default(),
            clicks: Vec::new(),
            show_heatmap: false,
            best: load_best(),
            history: Vec::new(),
        }
    }

    pub fn scn(&self) -> Scenario {
        SCENARIOS[self.scenario]
    }
}

// xorshift64 (free fns so we can split-borrow State)
pub fn rng_u64(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x
}
pub fn rng_f(s: &mut u64) -> f64 {
    (rng_u64(s) >> 11) as f64 / ((1u64 << 53) as f64) // 0.0..1.0
}
pub fn rng_range(s: &mut u64, lo: f64, hi: f64) -> f64 {
    lo + rng_f(s) * (hi - lo)
}

// ── 점수/정확도 ──
pub fn accuracy_pct(st: &State, mode: Mode) -> f64 {
    match mode {
        Mode::Click => {
            let total = st.stats.hits + st.stats.misclicks;
            if total > 0 {
                st.stats.hits as f64 / total as f64 * 100.0
            } else {
                0.0
            }
        }
        Mode::Track => {
            if st.stats.track_total > 0.0 {
                st.stats.track_on / st.stats.track_total * 100.0
            } else {
                0.0
            }
        }
    }
}

pub fn round_score(st: &State, _mode: Mode) -> f64 {
    st.stats.score
}
