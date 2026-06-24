//! 트래킹 타깃 이동 패턴 (스트레이프 / 에어 / 트레이서 블링크 / 파라 아크 / 겐지 대시).
use crate::model::{rng_range, Pattern, Target};

/// 트래킹 타깃 한 마리를 이동시킨다(패턴별). dt 초.
pub fn step_target(
    t: &mut Target,
    pat: Pattern,
    dt: f64,
    w: f64,
    h: f64,
    speed: f64,
    rng: &mut u64,
) {
    let m = t.r as f64;
    let bounce_x = |t: &mut Target| {
        if t.x < m {
            t.x = m;
            t.vx = t.vx.abs();
        } else if t.x > w - m {
            t.x = w - m;
            t.vx = -t.vx.abs();
        }
    };
    let bounce_y = |t: &mut Target| {
        if t.y < m {
            t.y = m;
            t.vy = t.vy.abs();
        } else if t.y > h - m {
            t.y = h - m;
            t.vy = -t.vy.abs();
        }
    };
    match pat {
        Pattern::Static => {}
        Pattern::Strafe => {
            t.phase -= dt;
            if t.phase <= 0.0 {
                // 방향 반전 + 약간의 속도 변동
                let s = speed * rng_range(rng, 0.7, 1.3);
                t.vx = if t.vx >= 0.0 { -s } else { s };
                t.phase = rng_range(rng, 0.25, 0.8);
            }
            t.x += t.vx * dt;
            bounce_x(t);
            t.y = t.base_y; // 수평 스트레이프
        }
        Pattern::Air => {
            // 수평 왕복 + 수직 사인 부유
            t.phase -= dt;
            if t.phase <= 0.0 {
                let s = speed * rng_range(rng, 0.6, 1.1);
                t.vx = if t.vx >= 0.0 { -s } else { s };
                t.phase = rng_range(rng, 0.5, 1.1);
            }
            t.x += t.vx * dt;
            bounce_x(t);
            t.t += dt;
            let amp = (h * 0.18).max(8.0);
            t.y = (t.base_y + amp * (t.t * 2.2).sin()).clamp(m, h - m);
        }
        Pattern::Blink => {
            // 느린 드리프트 + 주기적 순간이동
            t.phase -= dt;
            if t.phase <= 0.0 {
                t.x = rng_range(rng, m, w - m);
                t.y = rng_range(rng, m, h - m);
                let ang = rng_range(rng, 0.0, std::f64::consts::TAU);
                let s = speed * 0.35;
                t.vx = s * ang.cos();
                t.vy = s * ang.sin();
                t.phase = rng_range(rng, 0.45, 0.95);
            }
            t.x += t.vx * dt;
            t.y += t.vy * dt;
            bounce_x(t);
            bounce_y(t);
        }
        Pattern::Arc => {
            // 느린 큰 아크 (상단 위주)
            t.t += dt;
            let cx = w * 0.5;
            let ax = w * 0.4;
            let ay = (h * 0.28).max(10.0);
            t.x = (cx + ax * (t.t * 0.55).sin()).clamp(m, w - m);
            t.y = (t.base_y + ay * (t.t * 0.9 + 1.0).sin()).clamp(m, h - m);
        }
        Pattern::Dash => {
            // 정지 ↔ 빠른 직선 버스트
            t.phase -= dt;
            if t.phase <= 0.0 {
                let dashing = t.vx != 0.0 || t.vy != 0.0;
                if dashing {
                    t.vx = 0.0;
                    t.vy = 0.0;
                    t.phase = rng_range(rng, 0.3, 0.6); // 정지
                } else {
                    let ang = rng_range(rng, 0.0, std::f64::consts::TAU);
                    let s = speed * 2.2;
                    t.vx = s * ang.cos();
                    t.vy = s * ang.sin();
                    t.phase = rng_range(rng, 0.12, 0.22); // 대시
                }
            }
            t.x += t.vx * dt;
            t.y += t.vy * dt;
            bounce_x(t);
            bounce_y(t);
        }
    }
}
