use crate::{
    bot::{
        behavior::{
            aim::AimOnTarget,
            death::{IsDead, StayDead},
            find::FindTarget,
            melee::{CanMeleeAttack, DoMeleeAttack},
            movement::MoveToTarget,
            shoot::{CanShootTarget, ShootTarget},
            threat::{NeedsThreatenTarget, ThreatenTarget},
        },
        state_machine::StateMachine,
        BotHostility, Target,
    },
    character::Character,
    sound::SoundManager,
    utils::{BodyImpactHandler, ResourceProxy},
    MessageSender,
};
use fyrox::{
    core::{math::SmoothAngle, pool::Handle, visitor::prelude::*},
    scene::{node::Node, sound::SoundBufferResource, Scene},
    script::ScriptMessageSender,
    utils::{
        behavior::{
            composite::{CompositeNode, CompositeNodeKind},
            leaf::LeafNode,
            Behavior, BehaviorTree, Status,
        },
        navmesh::NavmeshAgent,
    },
};

pub mod aim;
pub mod death;
pub mod find;
pub mod melee;
pub mod movement;
pub mod shoot;
pub mod threat;

#[derive(Debug, PartialEq, Visit, Clone, Default)]
pub enum Action {
    #[default]
    Unknown,
    IsDead(IsDead),
    StayDead(StayDead),
    FindTarget(FindTarget),
    MoveToTarget(MoveToTarget),
    CanMeleeAttack(CanMeleeAttack),
    AimOnTarget(AimOnTarget),
    DoMeleeAttack(DoMeleeAttack),
    CanShootTarget(CanShootTarget),
    ShootTarget(ShootTarget),
    NeedsThreatenTarget(NeedsThreatenTarget),
    ThreatenTarget(ThreatenTarget),
}

impl<'a> Behavior<'a> for Action {
    type Context = BehaviorContext<'a>;

    fn tick(&mut self, context: &mut Self::Context) -> Status {
        match self {
            Action::Unknown => unreachable!(),
            Action::FindTarget(v) => v.tick(context),
            Action::MoveToTarget(v) => v.tick(context),
            Action::DoMeleeAttack(v) => v.tick(context),
            Action::ShootTarget(v) => v.tick(context),
            Action::CanMeleeAttack(v) => v.tick(context),
            Action::IsDead(v) => v.tick(context),
            Action::StayDead(v) => v.tick(context),
            Action::AimOnTarget(v) => v.tick(context),
            Action::CanShootTarget(v) => v.tick(context),
            Action::NeedsThreatenTarget(v) => v.tick(context),
            Action::ThreatenTarget(v) => v.tick(context),
        }
    }
}

pub struct BehaviorContext<'a> {
    pub scene: &'a mut Scene,
    pub actors: &'a [Handle<Node>],
    pub bot_handle: Handle<Node>,
    pub sender: &'a MessageSender,
    pub dt: f32,
    pub elapsed_time: f32,
    pub state_machine: &'a StateMachine,
    pub target: &'a mut Option<Target>,
    pub character: &'a mut Character,
    pub agent: &'a mut NavmeshAgent,
    pub impact_handler: &'a BodyImpactHandler,
    pub model: Handle<Node>,
    pub restoration_time: f32,
    pub v_recoil: &'a mut SmoothAngle,
    pub h_recoil: &'a mut SmoothAngle,
    pub move_speed: f32,
    pub threaten_timeout: &'a mut f32,
    pub sound_manager: &'a SoundManager,
    pub animation_player: Handle<Node>,
    pub script_message_sender: &'a ScriptMessageSender,
    pub navmesh: Handle<Node>,
    pub hostility: BotHostility,
    pub h_aim_angle_hack: f32,
    pub v_aim_angle_hack: f32,
    pub attack_sounds: &'a [ResourceProxy<SoundBufferResource>],

    // Output
    pub attack_animation_index: usize,
    pub movement_speed_factor: f32,
    pub is_moving: bool,
    pub is_attacking: bool,
    pub is_aiming_weapon: bool,
    pub is_screaming: bool,
}

#[derive(Default, Debug, Visit, Clone)]
pub struct BotBehavior {
    pub tree: BehaviorTree<Action>,
}

impl BotBehavior {
    pub fn new(spine: Handle<Node>, close_combat_distance: f32) -> Self {
        let mut tree = BehaviorTree::new();

        let entry = CompositeNode::new_selector(vec![
            CompositeNode::new_sequence(vec![
                IsDead::new_action(&mut tree),
                StayDead::new_action(&mut tree),
            ])
            .add_to(&mut tree),
            CompositeNode::new_sequence(vec![
                LeafNode::new(Action::FindTarget(FindTarget::default())).add_to(&mut tree),
                CompositeNode::new_sequence(vec![
                    LeafNode::new(AimOnTarget::new_action(spine)).add_to(&mut tree),
                    CompositeNode::new(
                        CompositeNodeKind::Selector,
                        vec![
                            CompositeNode::new_sequence(vec![
                                LeafNode::new(Action::NeedsThreatenTarget(
                                    NeedsThreatenTarget::default(),
                                ))
                                .add_to(&mut tree),
                                LeafNode::new(Action::ThreatenTarget(ThreatenTarget::default()))
                                    .add_to(&mut tree),
                            ])
                            .add_to(&mut tree),
                            CompositeNode::new_sequence(vec![
                                LeafNode::new(Action::CanShootTarget(CanShootTarget))
                                    .add_to(&mut tree),
                                LeafNode::new(Action::MoveToTarget(MoveToTarget {
                                    min_distance: 4.0,
                                }))
                                .add_to(&mut tree),
                                LeafNode::new(Action::ShootTarget(ShootTarget)).add_to(&mut tree),
                            ])
                            .add_to(&mut tree),
                            CompositeNode::new_sequence(vec![
                                LeafNode::new(Action::MoveToTarget(MoveToTarget {
                                    min_distance: close_combat_distance,
                                }))
                                .add_to(&mut tree),
                                LeafNode::new(Action::CanMeleeAttack(CanMeleeAttack))
                                    .add_to(&mut tree),
                                LeafNode::new(Action::DoMeleeAttack(DoMeleeAttack::default()))
                                    .add_to(&mut tree),
                            ])
                            .add_to(&mut tree),
                        ],
                    )
                    .add_to(&mut tree),
                ])
                .add_to(&mut tree),
            ])
            .add_to(&mut tree),
        ])
        .add_to(&mut tree);

        tree.set_entry_node(entry);

        Self { tree }
    }
}
