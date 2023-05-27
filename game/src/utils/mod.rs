use fyrox::{
    animation::{
        machine::{MachineLayer, PoseNode, State},
        Animation, AnimationContainer,
    },
    asset::core::rand::Rng,
    core::{
        algebra::{Point3, Unit, UnitQuaternion, Vector3},
        log::Log,
        pool::Handle,
        reflect::prelude::*,
        visitor::prelude::*,
    },
    rand,
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        animation::AnimationPlayer,
        graph::Graph,
        node::Node,
        sound::{self, context::SoundContext},
        Scene,
    },
};
use std::{collections::HashMap, fmt::Debug};

pub mod model_map;

#[derive(Clone, Debug)]
struct ImpactEntry {
    k: f32,
    source: UnitQuaternion<f32>,
}

#[derive(Default, Debug, Clone)]
pub struct BodyImpactHandler {
    additional_rotations: HashMap<Handle<Node>, ImpactEntry>,
}

impl BodyImpactHandler {
    pub fn handle_impact(
        &mut self,
        scene: &Scene,
        handle: Handle<Node>,
        impact_point: Vector3<f32>,
        direction: Vector3<f32>,
    ) {
        if let Some(node) = scene.graph.try_get(handle) {
            let global_transform = node.global_transform().try_inverse().unwrap_or_default();
            let local_impact_point = global_transform.transform_point(&Point3::from(impact_point));
            let local_direction = global_transform.transform_vector(&direction);
            // local_impact_point can be directly be used as vector because it is in
            // local coordinates of rigid body.
            if let Some(axis) = local_impact_point
                .coords
                .cross(&local_direction)
                .try_normalize(f32::EPSILON)
            {
                let additional_rotation = UnitQuaternion::from_axis_angle(
                    &Unit::new_normalize(axis),
                    24.0f32.to_radians(),
                );
                self.additional_rotations
                    .entry(handle)
                    .and_modify(|r| {
                        r.source = additional_rotation;
                        r.k = 0.0;
                    })
                    .or_insert(ImpactEntry {
                        k: 0.0,
                        source: additional_rotation,
                    });
            }
        } else {
            Log::warn(format!("[Impact Handler]: invalid handle {handle}!"))
        }
    }

    pub fn update_and_apply(&mut self, dt: f32, scene: &mut Scene) {
        for (body, entry) in self.additional_rotations.iter_mut() {
            let additional_rotation = entry.source.nlerp(&UnitQuaternion::default(), entry.k);
            entry.k += dt;
            let transform = scene.graph[*body].local_transform_mut();
            let new_rotation = **transform.rotation() * additional_rotation;
            transform.set_rotation(new_rotation);
        }
        self.additional_rotations.retain(|_, e| e.k < 1.0);
    }

    pub fn is_affected(&self, handle: Handle<Node>) -> bool {
        self.additional_rotations.contains_key(&handle)
    }
}

pub fn use_hrtf(context: &mut SoundContext) {
    let hrtf_sphere =
        fyrox::scene::sound::HrirSphere::from_file("data/sounds/hrtf.bin", sound::SAMPLE_RATE)
            .unwrap();
    context
        .state()
        .set_renderer(fyrox::scene::sound::Renderer::HrtfRenderer(
            fyrox::scene::sound::HrtfRenderer::new(hrtf_sphere),
        ));
}

pub fn create_play_animation_state(
    animation_resource: ModelResource,
    name: &str,
    layer: &mut MachineLayer,
    scene: &mut Scene,
    model: Handle<Node>,
    animation_player: Handle<Node>,
) -> (Handle<Animation>, Handle<State>) {
    let animation = *animation_resource
        .retarget_animations_to_player(model, animation_player, &mut scene.graph)
        .get(0)
        .unwrap();
    let node = layer.add_node(PoseNode::make_play_animation(animation));
    let state = layer.add_state(State::new(name, node));
    (animation, state)
}

pub fn is_probability_event_occurred(probability: f32) -> bool {
    rand::thread_rng().gen_range(0.0..1.0) < probability.clamp(0.0, 1.0)
}

pub fn fetch_animation_container_ref(graph: &Graph, handle: Handle<Node>) -> &AnimationContainer {
    graph
        .try_get_of_type::<AnimationPlayer>(handle)
        .unwrap()
        .animations()
}

pub fn fetch_animation_container_mut(
    graph: &mut Graph,
    handle: Handle<Node>,
) -> &mut AnimationContainer {
    graph
        .try_get_mut_of_type::<AnimationPlayer>(handle)
        .unwrap()
        .animations_mut()
        .get_value_mut_silent()
}

// To bypass https://github.com/FyroxEngine/Fyrox/issues/357.
#[derive(Reflect, Clone, Default, Visit, Debug, PartialEq)]
pub struct ResourceProxy<T>(pub Option<T>)
where
    T: Reflect + Clone + Default + Visit + Debug + PartialEq;
