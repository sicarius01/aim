// Aim Trainer — winsafe(Win32) 기반 개인용 에임 연습 프로그램
//
// 모듈 구성:
//   raw      : user32/dwmapi 직접 FFI (raw input, PostMessage, DwmFlush, GetSystemMetrics)
//   theme    : 색상 팔레트 + GDI 헬퍼(rgb, leak_brush)
//   model    : 모드/시나리오/설정/타깃/상태 + RNG + 점수 헬퍼
//   patterns : 트래킹 타깃 이동 패턴(스트레이프/에어/트레이서/파라/겐지)
//   persist  : 로컬 최고기록 + CSV 로그
//   render   : 에임 영역 장면 렌더링(이중 버퍼)
//   ui       : 위젯 생성 + 정적 패널 레이아웃
//   app      : 이벤트 배선·세션·raw input·vsync 프레임 오케스트레이션
//
// 렌더: vsync 페이서 스레드(DwmFlush)가 매 vblank 프레임을 그려 주사율대로 부드럽게.
// 감도: 게임 yaw + 모니터 픽셀/° 로 cm/360을 게임과 맞춰 실제 손맛으로 연습.

#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

mod app;
mod model;
mod patterns;
mod persist;
mod raw;
mod render;
mod theme;
mod ui;

fn main() {
    let app = app::App::new();
    if let Err(e) = app.run() {
        eprintln!("error: {}", e);
    }
}
