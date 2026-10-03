use capnp::message::{Builder, HeapAllocator};
use openpilot_card::{core::StateTail, cruise::CruiseCarrot};
use openpilot_cereal::{
    car_capnp::{car_params, car_state},
    log_capnp::event,
};
use openpilot_messaging::state::{Options, State};
use openpilot_params::Params;

struct Fixture {
    tail: CruiseCarrot,
    sm: State,
    _root: tempfile::TempDir,
    now: f64,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let params = Params::open(root.path(), "d").unwrap();
        for (key, value) in [
            ("CruiseSpeedUnit", "10"),
            ("CruiseSpeedUnitBasic", "1"),
            ("CruiseButtonLongDelay", "40"),
            ("AutoCruiseControl", "1"),
            ("AutoGasCancelSpeed", "30"),
        ] {
            params.put(key, value.as_bytes()).unwrap();
        }
        let mut cp = Builder::new_default();
        cp.init_root::<car_params::Builder>();
        let tail =
            CruiseCarrot::new(cp.get_root_as_reader().unwrap(), params, root.path(), 10.).unwrap();
        let sm = State::new(
            &[
                "carControl",
                "carrotMan",
                "longitudinalPlan",
                "radarState",
                "drivingModelData",
            ],
            Options {
                simulation: true,
                ..Options::default()
            },
        )
        .unwrap();
        Self {
            tail,
            sm,
            _root: root,
            now: 10.,
        }
    }
    fn state(
        &self,
        button: Option<(car_state::button_event::Type, bool)>,
    ) -> Builder<HeapAllocator> {
        let mut message = Builder::new_default();
        let mut cs = message.init_root::<car_state::Builder>();
        cs.set_v_ego(80. / 3.6);
        cs.set_v_ego_cluster(80. / 3.6);
        cs.set_can_valid(true);
        cs.set_gear_shifter(car_state::GearShifter::Drive);
        cs.reborrow().init_cruise_state().set_available(true);
        if let Some((kind, pressed)) = button {
            let mut b = cs.init_button_events(1).get(0);
            b.set_type(kind);
            b.set_pressed(pressed);
        }
        message
    }
    fn step(&mut self, cs: &mut Builder<HeapAllocator>) -> serde_json::Value {
        self.now += 0.031;
        let mut cc = Builder::new_default();
        cc.init_root::<event::Builder>()
            .init_car_control()
            .set_enabled(true);
        self.sm
            .update(self.now, &[capnp::serialize::write_message_to_words(&cc)])
            .unwrap();
        self.tail
            .update_at(cs.get_root_as_reader().unwrap(), &self.sm, true, self.now)
            .unwrap();
        self.tail.project(cs.get_root().unwrap()).unwrap();
        serde_json::to_value(self.tail.snapshot()).unwrap()
    }
}

#[test]
fn short_accel_increases_active_speed_after_release() {
    let mut f = Fixture::new();
    let mut initial = f.state(None);
    f.step(&mut initial);
    let mut pressed = f.state(Some((car_state::button_event::Type::AccelCruise, true)));
    f.step(&mut pressed);
    let mut released = f.state(Some((car_state::button_event::Type::AccelCruise, false)));
    f.step(&mut released);
    assert_eq!(
        released
            .get_root_as_reader::<car_state::Reader>()
            .unwrap()
            .get_v_cruise(),
        81.
    );
}

#[test]
fn dedicated_set_selects_current_speed_without_adding_an_accel_step() {
    let mut f = Fixture::new();
    let mut initial = f.state(None);
    f.step(&mut initial);
    let mut cs = f.state(Some((car_state::button_event::Type::SetCruise, false)));
    cs.get_root::<car_state::Builder>()
        .unwrap()
        .set_v_ego_cluster(50. / 3.6);
    f.step(&mut cs);
    assert_eq!(
        cs.get_root_as_reader::<car_state::Reader>()
            .unwrap()
            .get_v_cruise(),
        50.
    );
}

#[test]
fn initialize_preserves_source_no_op() {
    let mut f = Fixture::new();
    let mut cs = f.state(None);
    f.step(&mut cs);
    let before = serde_json::to_value(f.tail.snapshot()).unwrap();
    f.tail
        .initialize(cs.get_root_as_reader().unwrap(), true)
        .unwrap();
    assert_eq!(serde_json::to_value(f.tail.snapshot()).unwrap(), before);
}

#[test]
fn projection_preserves_unrelated_vehicle_fields() {
    let mut f = Fixture::new();
    let mut cs = f.state(None);
    cs.get_root::<car_state::Builder>()
        .unwrap()
        .set_engine_rpm(2345.);
    f.step(&mut cs);
    assert_eq!(
        cs.get_root_as_reader::<car_state::Reader>()
            .unwrap()
            .get_engine_rpm(),
        2345.
    );
}

#[test]
fn gap_press_queues_personality_write() {
    let mut f = Fixture::new();
    let mut cs = f.state(None);
    f.step(&mut cs);
    let mut pressed = f.state(Some((car_state::button_event::Type::GapAdjustCruise, true)));
    f.step(&mut pressed);
    let mut released = f.state(Some((
        car_state::button_event::Type::GapAdjustCruise,
        false,
    )));
    f.step(&mut released);
    assert_eq!(
        f.tail.take_param_writes(),
        vec![("LongitudinalPersonality".to_owned(), b"2".to_vec())]
    );
}
