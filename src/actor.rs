use crate::bot::BotKind;
use crate::{bot::Bot, character::Character, level::UpdateContext, player::Player};
use fyrox::{
    core::{
        algebra::Vector3,
        pool::{Handle, Pool},
        visitor::{Visit, VisitResult, Visitor},
    },
    resource::texture::Texture,
    scene::Scene,
};
use std::ops::{Deref, DerefMut};

#[allow(clippy::large_enum_variant)]
#[derive(Visit)]
pub enum Actor {
    Bot(Bot),
    Player(Player),
}

impl Default for Actor {
    fn default() -> Self {
        Actor::Bot(Default::default())
    }
}

macro_rules! static_dispatch {
    ($self:ident, $func:ident, $($args:expr),*) => {
        match $self {
            Actor::Player(v) => v.$func($($args),*),
            Actor::Bot(v) => v.$func($($args),*),
        }
    };
}

impl Actor {
    pub fn can_be_removed(&self, scene: &Scene) -> bool {
        static_dispatch!(self, can_be_removed, scene)
    }

    pub fn clean_up(&mut self, scene: &mut Scene) {
        static_dispatch!(self, clean_up, scene)
    }
}

impl Deref for Actor {
    type Target = Character;

    fn deref(&self) -> &Self::Target {
        match self {
            Actor::Bot(v) => v,
            Actor::Player(v) => v,
        }
    }
}

impl DerefMut for Actor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Actor::Bot(v) => v,
            Actor::Player(v) => v,
        }
    }
}

pub enum TargetKind {
    Player,
    Bot(BotKind),
}

// Helper struct that used to hold information about possible target for bots
// it contains all needed information to select suitable target. This is needed
// because of borrowing rules that does not allows to have a mutable reference
// to array element and iterate over array using immutable borrow.
pub struct TargetDescriptor {
    pub handle: Handle<Actor>,
    pub health: f32,
    pub position: Vector3<f32>,
    pub kind: TargetKind,
}

#[derive(Default, Visit)]
pub struct ActorContainer {
    pool: Pool<Actor>,
    #[visit(skip)]
    target_descriptors: Vec<TargetDescriptor>,
}

impl ActorContainer {
    pub fn new() -> Self {
        Self {
            pool: Default::default(),
            target_descriptors: Default::default(),
        }
    }

    pub fn add(&mut self, actor: Actor) -> Handle<Actor> {
        self.pool.spawn(actor)
    }

    pub fn get(&self, actor: Handle<Actor>) -> &Actor {
        self.pool.borrow(actor)
    }

    pub fn try_get(&self, actor: Handle<Actor>) -> Option<&Actor> {
        self.pool.try_borrow(actor)
    }

    pub fn contains(&self, actor: Handle<Actor>) -> bool {
        self.pool.is_valid_handle(actor)
    }

    pub fn get_mut(&mut self, actor: Handle<Actor>) -> &mut Actor {
        self.pool.borrow_mut(actor)
    }

    pub fn free(&mut self, actor_handle: Handle<Actor>) {
        for actor in self.pool.iter_mut() {
            if let Actor::Bot(bot) = actor {
                bot.on_actor_removed(actor_handle);
            }
        }

        self.pool.free(actor_handle);
    }

    pub fn count(&self) -> u32 {
        self.pool.alive_count()
    }

    pub fn update(&mut self, context: &mut UpdateContext) {
        self.target_descriptors.clear();
        for (handle, actor) in self.pool.pair_iter() {
            if !actor.is_dead() {
                self.target_descriptors.push(TargetDescriptor {
                    handle,
                    health: actor.health,
                    position: actor.position(&context.scene.graph),
                    kind: match actor {
                        Actor::Bot(bot) => TargetKind::Bot(bot.kind),
                        Actor::Player(_) => TargetKind::Player,
                    },
                });
            }
        }

        for (handle, actor) in self.pool.pair_iter_mut() {
            match actor {
                Actor::Bot(bot) => bot.update(handle, context, &self.target_descriptors),
                Actor::Player(player) => player.update(handle, context),
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Actor> {
        self.pool.iter()
    }

    pub fn pair_iter(&self) -> impl Iterator<Item = (Handle<Actor>, &Actor)> {
        self.pool.pair_iter()
    }

    pub fn pair_iter_mut(&mut self) -> impl Iterator<Item = (Handle<Actor>, &mut Actor)> {
        self.pool.pair_iter_mut()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Actor> {
        self.pool.iter_mut()
    }

    pub fn resolve(
        &mut self,
        scene: &mut Scene,
        display_texture: Texture,
        inventory_texture: Texture,
        item_texture: Texture,
        journal_texture: Texture,
    ) {
        for actor in self.pool.iter_mut() {
            match actor {
                Actor::Player(player) => {
                    player.resolve(
                        scene,
                        display_texture.clone(),
                        inventory_texture.clone(),
                        item_texture.clone(),
                        journal_texture.clone(),
                    );
                }
                Actor::Bot(bot) => {
                    bot.resolve();
                }
            }

            actor.restore_hit_boxes(scene);
        }
    }
}
