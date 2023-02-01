use crate::actor::TimerActor;
use crate::actor::TimerId;
use crate::SetTimeout;
use crate::Timeout;
use std::convert::Infallible;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::NoConfig;
use tedge_actors::ServiceMessageBoxBuilder;

pub struct TimerActorBuilder {
    box_builder: ServiceMessageBoxBuilder<SetTimeout<TimerId>, Timeout<TimerId>>,
}

impl Default for TimerActorBuilder {
    fn default() -> Self {
        TimerActorBuilder {
            box_builder: ServiceMessageBoxBuilder::new("Timer", 16),
        }
    }
}

impl Builder<(TimerActor, <TimerActor as Actor>::MessageBox)> for TimerActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<(TimerActor, <TimerActor as Actor>::MessageBox), Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> (TimerActor, <TimerActor as Actor>::MessageBox) {
        let actor = TimerActor::default();
        let actor_box = self.box_builder.build();
        (actor, actor_box)
    }
}

impl MessageBoxSocket<SetTimeout<TimerId>, Timeout<TimerId>, NoConfig> for TimerActorBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPlug<SetTimeout<TimerId>, Timeout<TimerId>>,
        config: NoConfig,
    ) {
        self.box_builder.connect_with(peer, config);
    }
}
