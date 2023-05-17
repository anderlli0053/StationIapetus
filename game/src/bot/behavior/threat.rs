//! Bots can threaten the player before attack, this mod has behavior nodes for this.

use crate::bot::behavior::BehaviorContext;
use fyrox::{
    core::{rand::Rng, visitor::prelude::*},
    rand,
    utils::behavior::{Behavior, Status},
};

#[derive(Default, Debug, PartialEq, Visit, Eq, Clone)]
pub struct ThreatenTarget {
    in_progress: bool,
}

impl<'a> Behavior<'a> for ThreatenTarget {
    type Context = BehaviorContext<'a>;

    fn tick(&mut self, ctx: &mut Self::Context) -> Status {
        if let Some(upper_body_layer) = ctx.state_machine.upper_body_layer(&ctx.scene.graph) {
            if upper_body_layer.active_state() == ctx.state_machine.threaten_state {
                self.in_progress = true;
                ctx.character.stand_still(&mut ctx.scene.graph);
                return Status::Running;
            } else if self.in_progress {
                self.in_progress = false;
                *ctx.threaten_timeout = rand::thread_rng().gen_range(20.0..60.0);
                return Status::Success;
            } else {
                ctx.is_screaming = true;
                return Status::Running;
            }
        } else {
            Status::Failure
        }
    }
}

#[derive(Default, Debug, PartialEq, Visit, Eq, Clone)]
pub struct NeedsThreatenTarget;

impl<'a> Behavior<'a> for NeedsThreatenTarget {
    type Context = BehaviorContext<'a>;

    fn tick(&mut self, context: &mut Self::Context) -> Status {
        if *context.threaten_timeout <= 0.0 {
            Status::Success
        } else {
            Status::Failure
        }
    }
}
