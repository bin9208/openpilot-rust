//! Optional conflated scene context, kept separate from nonconflated CAN/ego joins.
use crate::{
    batch::Ego,
    decoder::{
        hyundai::bosch::{
            context::{Message, Pose},
            model_snapshot::Input,
        },
        Interface,
    },
    wire, Error,
};
use openpilot_can::Packet;
use openpilot_messaging::services;
use openpilot_msgq::{MultiSubscriber, Subscription};
pub(super) struct Context {
    subscriptions: MultiSubscriber,
    pose: Message<Pose>,
    model: Message<Input>,
    replay: bool,
    replay_ns: u64,
}
impl Context {
    pub fn new(replay: bool) -> Result<Self, Error> {
        let specifications = ["livePose", "modelV2"].map(|name| {
            services::lookup(name)
                .map(|s| Subscription {
                    endpoint: name,
                    capacity: s.queue_size,
                    // Admission flag for this private subscriber group; it is never
                    // included in the CAN/ego poll and only receives nonblocking.
                    polled: true,
                })
                .ok_or(Error::Contract("Bosch context IPC service absent"))
        });
        let [pose, model] = specifications;
        Ok(Self {
            subscriptions: MultiSubscriber::for_runtime(&[pose?, model?])?,
            pose: Message::default(),
            model: Message::default(),
            replay,
            replay_ns: 0,
        })
    }
    pub fn update(&mut self) -> Result<(), Error> {
        if let Some(bytes) = self.subscriptions.receive_one(0)? {
            self.pose = wire::bosch_pose(&bytes)?;
        }
        if let Some(bytes) = self.subscriptions.receive_one(1)? {
            self.model = wire::bosch_model(&bytes)?;
        }
        Ok(())
    }
    pub fn apply(
        &mut self,
        radar: &mut Interface,
        ego: Ego,
        packets: &[Packet],
    ) -> Result<(), Error> {
        let now = if self.replay {
            if !packets.is_empty() {
                self.replay_ns = self.replay_ns.max(ego.last_can_ns);
            }
            self.replay_ns
        } else {
            ego.receive_ns
        };
        radar.set_bosch_context(
            i64::try_from(now).map_err(|_| Error::IntegerOverflow)?,
            self.pose.clone(),
            self.model.clone(),
        );
        Ok(())
    }
}
