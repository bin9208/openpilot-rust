use super::{
    accept, first, Config, ConnectionState, Error, Reply, Rig, Value, ATTR_ICE_CONTROLLED,
    ATTR_ICE_CONTROLLING, ATTR_USE_CANDIDATE, CODE_BAD_REQUEST, CODE_ROLE_CONFLICT,
};

pub(super) fn role_retry(nomination_error: bool) -> Result<Value, Error> {
    let mut rig = Rig::new(Config {
        controlling: false,
        ..Default::default()
    })?;
    let first_check = first(&mut rig)?;
    assert!(first_check.message.contains(ATTR_ICE_CONTROLLED));
    rig.response(
        &first_check,
        first_check.message.transaction_id,
        Reply::Error(CODE_ROLE_CONFLICT),
    )?;
    assert!(rig.read(0)?.is_none());
    let retry = first(&mut rig)?;
    assert!(retry.message.contains(ATTR_ICE_CONTROLLING));
    rig.response(
        &retry,
        first_check.message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Checking);
    accept(&mut rig, &retry)?;
    let nomination = first(&mut rig)?;
    assert!(nomination.message.contains(ATTR_USE_CANDIDATE));
    if nomination_error {
        rig.response(
            &nomination,
            nomination.message.transaction_id,
            Reply::Error(CODE_BAD_REQUEST),
        )?;
        assert!(rig.read(0)?.is_none());
        assert_eq!(rig.agent.state(), ConnectionState::Failed);
    } else {
        accept(&mut rig, &nomination)?;
        assert_eq!(rig.agent.state(), ConnectionState::Connected);
    }
    rig.finish(if nomination_error {
        "487 role retry; unnominated success then nomination400 fails"
    } else {
        "487 role retry; stale pre-retry error ignored; nominated connection"
    })
}

pub(super) fn nomination_fallback() -> Result<Value, Error> {
    let mut rig = Rig::new(Config {
        remotes: 2,
        controlling: false,
        ..Default::default()
    })?;
    let original = rig.writes()?;
    assert_eq!(original.len(), 2);
    rig.response(
        &original[0],
        original[0].message.transaction_id,
        Reply::Error(CODE_ROLE_CONFLICT),
    )?;
    assert!(rig.read(0)?.is_none());
    let retry = rig.writes()?;
    assert_eq!(retry.len(), 2);
    accept(&mut rig, &retry[0])?;
    let failed_nomination = first(&mut rig)?;
    assert_eq!(failed_nomination.remote, retry[0].remote);
    rig.response(
        &failed_nomination,
        failed_nomination.message.transaction_id,
        Reply::Error(CODE_BAD_REQUEST),
    )?;
    assert!(rig.read(0)?.is_none());
    assert_eq!(rig.agent.state(), ConnectionState::Checking);
    accept(&mut rig, &retry[1])?;
    let viable_nomination = first(&mut rig)?;
    assert_eq!(viable_nomination.remote, retry[1].remote);
    accept(&mut rig, &viable_nomination)?;
    assert_eq!(rig.agent.state(), ConnectionState::Connected);
    rig.finish("487 role retry; nomination400 chooses another viable pair")
}
