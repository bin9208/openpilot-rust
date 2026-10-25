//! 실제 C++ peer driver와 pure client 회귀가 공유하는 Planner40 송신 순서.
use super::scheduler_client::SchedulerClient;
use crate::seal::Result;
use serde::Serialize;

#[derive(Serialize)]
pub struct Timing {
    pub index: usize,
    pub target: f64,
    pub sent: f64,
    pub model_done: f64,
    pub live_done: f64,
}

/// Planner 전용 세 동작이다. 생산 구현은 native C++ peer와 실제 daemon을 사용한다.
pub trait FrameEvents {
    fn sleep_until(&mut self, target: f64) -> Result<()>;
    fn model(&mut self, index: usize) -> Result<(f64, f64)>;
    fn live(&mut self, index: usize) -> Result<f64>;
}
pub fn drive40(
    client: &mut SchedulerClient,
    events: &mut impl FrameEvents,
    started: f64,
) -> Result<Vec<Timing>> {
    let mut previous_done = started - 0.05;
    let mut previous_sent = previous_done;
    let mut timing = Vec::new();
    for index in 0..40 {
        let target = client.target(started + index as f64 * 0.05, previous_sent, previous_done)?;
        events.sleep_until(target)?;
        let (sent, model_done) = events.model(index)?;
        previous_sent = sent;
        previous_done = model_done;
        events.sleep_until(target + 0.025)?;
        let live_done = events.live(index)?;
        timing.push(Timing {
            index,
            target,
            sent,
            model_done,
            live_done,
        });
    }
    Ok(timing)
}
