#[path = "webrtc_stun_checks/rig.rs"]
mod rig;
#[path = "webrtc_stun_checks/roles.rs"]
mod roles;

use openpilot_webrtc::Error;
use rig::{Config, Reply, Request, Rig};
use rtc::sansio::Protocol;
use rtc::{
    ice::state::ConnectionState,
    stun::{
        attributes::{ATTR_ICE_CONTROLLED, ATTR_ICE_CONTROLLING, ATTR_USE_CANDIDATE},
        error_code::{CODE_BAD_REQUEST, CODE_ROLE_CONFLICT},
        message::TransactionId,
    },
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

fn first(rig: &mut Rig) -> Result<Request, Error> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(request) = rig.writes()?.into_iter().next() {
            return Ok(request);
        }
        if Instant::now() >= deadline {
            return Err(Error::Contract("owned pending request absent"));
        }
        let wake = rig
            .agent
            .poll_timeout()
            .ok_or(Error::Contract("provider timer absent"))?;
        std::thread::sleep(
            wake.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(200)),
        );
    }
}

fn accept(rig: &mut Rig, request: &Request) -> Result<(), Error> {
    rig.response(request, request.message.transaction_id, Reply::Success)?;
    assert!(rig.read(0)?.is_none());
    Ok(())
}

fn unknown() -> Result<Value, Error> {
    let mut rig = Rig::new(Config::default())?;
    let request = first(&mut rig)?;
    rig.response(
        &request,
        TransactionId([0xff; 12]),
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Checking);
    rig.response(
        &request,
        request.message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Failed);
    rig.finish("unknown ignored; matching checklist exhausted")
}

fn wrong_local() -> Result<Value, Error> {
    let mut rig = Rig::new(Config {
        locals: 2,
        ..Default::default()
    })?;
    let address = rig.locals[0].local_addr()?;
    let mut request = rig
        .writes()?
        .into_iter()
        .find(|request| request.from == address)
        .ok_or(Error::Contract("first owned local request absent"))?;
    rig.wrong_local(&mut request, 1)?;
    rig.response(
        &request,
        request.message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(1)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Checking);
    assert!(rig
        .agent
        .get_candidate_pairs_stats(Instant::now())
        .iter()
        .all(|pair| pair.state.to_string() == "in-progress"));
    rig.wrong_local(&mut request, 0)?;
    rig.response(
        &request,
        request.message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Checking);
    rig.finish("wrong local ignored; matching owner alone failed")
}

fn viable_and_late() -> Result<Value, Error> {
    let mut rig = Rig::new(Config {
        remotes: 2,
        ..Default::default()
    })?;
    let requests = rig.writes()?;
    assert_eq!(requests.len(), 2);
    rig.response(
        &requests[0],
        requests[0].message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Checking);
    accept(&mut rig, &requests[1])?;
    let nomination = first(&mut rig)?;
    assert!(nomination.message.contains(ATTR_USE_CANDIDATE));
    accept(&mut rig, &nomination)?;
    assert_eq!(rig.agent.state(), ConnectionState::Connected);
    rig.response(
        &nomination,
        requests[1].message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Connected);
    rig.finish("failed pair does not block viable pair; late completed transaction ignored")
}

fn bad_requests() -> Result<Value, Error> {
    let mut rig = Rig::new(Config::default())?;
    let _request = first(&mut rig)?;
    let checks = [
        rig.signed_bad_request(true)?,
        rig.signed_bad_request(false)?,
    ];
    assert!(checks
        .iter()
        .all(|value| value["error"] == 400 && value["transaction_matches"] == true));
    Ok(
        json!({"checks":checks,"transport":rig.finish("wrong username and bad integrity receive signed400")?}),
    )
}

fn ordinary() -> Result<Value, Error> {
    let mut rig = Rig::new(Config {
        source: false,
        ..Default::default()
    })?;
    let request = first(&mut rig)?;
    rig.response(
        &request,
        request.message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_some());
    assert_eq!(rig.agent.state(), ConnectionState::Checking);
    rig.finish("ordinary provider error policy remains unchanged")
}

fn main() -> Result<(), Error> {
    type Control = fn() -> Result<Value, Error>;
    let controls: [(&str, Control); 8] = [
        ("unknown", unknown),
        ("wrong-local", wrong_local),
        ("viable-late", viable_and_late),
        ("role-retry", || roles::role_retry(false)),
        ("nomination-error", || roles::role_retry(true)),
        ("bad-requests", bad_requests),
        ("ordinary", ordinary),
        ("nomination-fallback", roles::nomination_fallback),
    ];
    let selected = std::env::args().nth(1);
    let mut results = Vec::new();
    for (name, run) in controls {
        if selected.as_deref().is_none_or(|selected| selected == name) {
            eprintln!("Running owned STUN control {name}");
            results.push(run()?);
        }
    }
    assert!(!results.is_empty());
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
