use crate::{
    character::{character_ref, Character, CharacterMessage, CharacterMessageData, DamageDealer},
    current_level_ref, effects,
    effects::EffectKind,
    game_ref,
    weapon::Hit,
    Turret, Weapon,
};
use fyrox::{
    core::{
        algebra::Vector3,
        math::{vector_to_quat, Vector3Ext},
        pool::Handle,
        reflect::prelude::*,
        uuid::{uuid, Uuid},
        visitor::prelude::*,
    },
    engine::resource_manager::ResourceManager,
    impl_component_provider,
    resource::model::Model,
    scene::{
        collider::Collider,
        graph::physics::FeatureId,
        node::{Node, TypeUuidProvider},
        rigidbody::RigidBody,
        sound::SoundBufferResource,
        Scene,
    },
    script::{ScriptContext, ScriptTrait},
};
use serde::Deserialize;
use strum_macros::{AsRefStr, EnumString, EnumVariantNames};

#[derive(
    Deserialize, Copy, Clone, Debug, Visit, Reflect, AsRefStr, EnumString, EnumVariantNames,
)]
pub enum Damage {
    Splash { radius: f32, amount: f32 },
    Point(f32),
}

impl Default for Damage {
    fn default() -> Self {
        Self::Point(0.0)
    }
}

impl Damage {
    #[must_use]
    pub fn scale(&self, k: f32) -> Self {
        match *self {
            Self::Splash { amount, radius } => Self::Splash {
                amount: amount * k.abs(),
                radius,
            },
            Self::Point(amount) => Self::Point(amount * k.abs()),
        }
    }

    pub fn amount(&self) -> f32 {
        *match self {
            Damage::Splash { amount, .. } => amount,
            Damage::Point(amount) => amount,
        }
    }
}

#[derive(Visit, Reflect, Debug, Clone)]
pub struct Projectile {
    #[reflect(hidden)]
    dir: Vector3<f32>,

    pub owner: Handle<Node>,

    #[reflect(hidden)]
    initial_velocity: Vector3<f32>,

    #[reflect(hidden)]
    last_position: Vector3<f32>,

    #[visit(optional)]
    use_ray_casting: bool,

    #[visit(optional)]
    speed: Option<f32>,

    #[visit(optional)]
    impact_effect: Option<Model>,

    #[visit(optional)]
    impact_sound: Option<SoundBufferResource>,

    #[visit(optional)]
    damage: Damage,

    // A handle to collider of the projectile. It is used as a cache to prevent searching for it
    // every frame.
    #[visit(skip)]
    #[reflect(hidden)]
    collider: Handle<Node>,
}

impl_component_provider!(Projectile);

impl TypeUuidProvider for Projectile {
    fn type_uuid() -> Uuid {
        uuid!("6b60c75e-83cf-406b-8106-e87d5ab98132")
    }
}

impl Default for Projectile {
    fn default() -> Self {
        Self {
            dir: Default::default(),
            owner: Default::default(),
            initial_velocity: Default::default(),
            last_position: Default::default(),
            use_ray_casting: true,
            speed: Some(10.0),
            impact_effect: None,
            impact_sound: None,
            damage: Default::default(),
            collider: Default::default(),
        }
    }
}

impl Projectile {
    pub fn add_to_scene(
        resource: &Model,
        scene: &mut Scene,
        dir: Vector3<f32>,
        position: Vector3<f32>,
        owner: Handle<Node>,
        initial_velocity: Vector3<f32>,
    ) -> Handle<Node> {
        let instance_handle = resource.instantiate(scene);

        let instance_ref = &mut scene.graph[instance_handle];

        instance_ref.local_transform_mut().set_position(position);

        if let Some(projectile) = instance_ref.try_get_script_mut::<Projectile>() {
            projectile.initial_velocity = initial_velocity;
            projectile.dir = dir.try_normalize(f32::EPSILON).unwrap_or_else(Vector3::y);
            projectile.owner = owner;
        }

        scene.graph.update_hierarchical_data();

        instance_handle
    }
}

impl ScriptTrait for Projectile {
    fn on_init(&mut self, context: &mut ScriptContext) {
        let node = &mut context.scene.graph[context.handle];

        self.last_position = node.global_position();

        if let Some(rigid_body) = node.cast_mut::<RigidBody>() {
            rigid_body.set_lin_vel(self.initial_velocity);
        }
    }

    fn on_start(&mut self, ctx: &mut ScriptContext) {
        self.collider = ctx
            .scene
            .graph
            .find(ctx.handle, &mut |n| {
                n.query_component_ref::<Collider>().is_some()
            })
            .map(|(h, _)| h)
            .unwrap_or_default();
    }

    fn on_update(&mut self, ctx: &mut ScriptContext) {
        let game = game_ref(ctx.plugins);
        let level = current_level_ref(ctx.plugins).unwrap();

        let position = ctx.scene.graph[ctx.handle].global_position();

        let mut hit = None;

        if self.use_ray_casting {
            hit = Weapon::ray_hit(
                self.last_position,
                position,
                self.owner,
                &current_level_ref(ctx.plugins).unwrap().actors,
                &mut ctx.scene.graph,
                // Ignore self collider.
                self.collider,
            );
            self.last_position = position;
        }

        if hit.is_none() {
            // Collect hits from self collider.
            if let Some(collider) = ctx.scene.graph.try_get_of_type::<Collider>(self.collider) {
                let owner_character =
                    ctx.scene
                        .graph
                        .try_get(self.owner)
                        .map_or(Default::default(), |owner_node| {
                            if let Some(weapon) = owner_node.try_get_script::<Weapon>() {
                                weapon.owner
                            } else if owner_node
                                .script()
                                .map(|s| s.query_component_ref::<Character>())
                                .is_some()
                            {
                                self.owner
                            } else {
                                Default::default()
                            }
                        });

                'contact_loop: for contact in collider.contacts(&ctx.scene.graph.physics) {
                    let other_collider = if self.collider == contact.collider1 {
                        contact.collider2
                    } else {
                        contact.collider1
                    };
                    for manifold in contact.manifolds {
                        for point in manifold.points {
                            for &actor_handle in level.actors.iter() {
                                let character = character_ref(actor_handle, &ctx.scene.graph);
                                for hit_box in character.hit_boxes.iter() {
                                    if hit_box.collider == other_collider {
                                        hit = Some(Hit {
                                            hit_actor: actor_handle,
                                            shooter_actor: owner_character,
                                            position: position
                                                + if self.collider == contact.collider1 {
                                                    point.local_p2
                                                } else {
                                                    point.local_p1
                                                },
                                            normal: manifold.normal,
                                            collider: other_collider,
                                            feature: FeatureId::Unknown,
                                            hit_box: Some(*hit_box),
                                            query_buffer: vec![],
                                        });

                                        break 'contact_loop;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(hit) = hit {
            let damage = self
                .damage
                .scale(hit.hit_box.map_or(1.0, |h| h.damage_factor));

            let critical_shot_probability =
                ctx.scene
                    .graph
                    .try_get(self.owner)
                    .map_or(0.0, |owner_node| {
                        if let Some(weapon) = owner_node.try_get_script::<Weapon>() {
                            weapon.definition.base_critical_shot_probability
                        } else if owner_node.has_script::<Turret>() {
                            0.01
                        } else {
                            0.0
                        }
                    });

            match damage {
                Damage::Splash { radius, amount } => {
                    let level = current_level_ref(ctx.plugins).unwrap();
                    // Just find out actors which must be damaged and re-cast damage message for each.
                    for &actor_handle in level.actors.iter() {
                        let character = character_ref(actor_handle, &ctx.scene.graph);
                        // TODO: Add occlusion test. This will hit actors through walls.
                        let character_position = character.position(&ctx.scene.graph);
                        if character_position.metric_distance(&position) <= radius {
                            ctx.message_sender.send_global(CharacterMessage {
                                character: actor_handle,
                                data: CharacterMessageData::Damage {
                                    dealer: DamageDealer {
                                        entity: hit.shooter_actor,
                                    },
                                    hitbox: None,
                                    /// TODO: Maybe collect all hitboxes?
                                    amount,
                                    critical_shot_probability,
                                },
                            });
                        }
                    }
                }
                Damage::Point(amount) => {
                    ctx.message_sender.send_global(CharacterMessage {
                        character: hit.hit_actor,
                        data: CharacterMessageData::Damage {
                            dealer: DamageDealer {
                                entity: hit.shooter_actor,
                            },
                            hitbox: hit.hit_box,
                            amount,
                            critical_shot_probability,
                        },
                    });
                }
            }

            effects::create(
                if hit.hit_actor.is_some() {
                    EffectKind::BloodSpray
                } else {
                    EffectKind::BulletImpact
                },
                &mut ctx.scene.graph,
                ctx.resource_manager,
                hit.position,
                vector_to_quat(hit.normal),
            );

            if let Some(impact_sound) = self.impact_sound.as_ref() {
                game.level
                    .as_ref()
                    .unwrap()
                    .sound_manager
                    .play_sound_buffer(
                        &mut ctx.scene.graph,
                        impact_sound,
                        hit.position,
                        1.0,
                        4.0,
                        3.0,
                    );
            }

            // Defer destruction.
            ctx.scene.graph[ctx.handle].set_lifetime(Some(0.0));
        }

        // Movement of kinematic projectiles is controlled explicitly.
        if let Some(speed) = self.speed {
            let total_velocity = self.dir.scale(speed);
            ctx.scene.graph[ctx.handle]
                .local_transform_mut()
                .offset(total_velocity);
        }

        // Reduce initial velocity down to zero over time. This is needed because projectile
        // stabilizes its movement over time.
        self.initial_velocity.follow(&Vector3::default(), 0.15);
    }

    fn restore_resources(&mut self, resource_manager: ResourceManager) {
        let mut state = resource_manager.state();

        let containers = state.containers_mut();

        containers
            .models
            .try_restore_optional_resource(&mut self.impact_effect);

        containers
            .sound_buffers
            .try_restore_optional_resource(&mut self.impact_sound);
    }

    fn id(&self) -> Uuid {
        Self::type_uuid()
    }
}
