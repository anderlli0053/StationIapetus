use crate::inventory::Inventory;
use crate::item::ItemKind;
use crate::{actor::ActorContainer, message::Message, Actor};
use rg3d::{
    core::{
        algebra::{Isometry3, Translation3, Vector3},
        color::Color,
        pool::{Handle, Pool},
        visitor::{Visit, VisitResult, Visitor},
    },
    scene::{graph::Graph, node::Node, Scene},
};
use std::ops::{Index, IndexMut};
use std::{path::PathBuf, sync::mpsc::Sender};

#[derive(Copy, Clone, Eq, PartialEq, Visit)]
#[repr(u32)]
pub enum DoorState {
    Opened = 0,
    Opening = 1,
    Closed = 2,
    Closing = 3,
    Locked = 4,
    Broken = 5,
}

impl Default for DoorState {
    fn default() -> Self {
        Self::Closed
    }
}

#[derive(Copy, Clone, Visit)]
#[repr(C)]
pub enum DoorDirection {
    Side,
    Up,
}

impl Default for DoorDirection {
    fn default() -> Self {
        Self::Side
    }
}

#[derive(Default, Visit)]
pub struct Door {
    node: Handle<Node>,
    lights: Vec<Handle<Node>>,
    state: DoorState,
    offset: f32,
    initial_position: Vector3<f32>,
    open_direction: DoorDirection,
    open_offset_amount: f32,
}

impl Door {
    pub fn new(
        node: Handle<Node>,
        graph: &Graph,
        state: DoorState,
        open_direction: DoorDirection,
        open_offset_amount: f32,
    ) -> Self {
        Self {
            node,
            lights: graph
                .traverse_handle_iter(node)
                .filter(|&handle| graph[handle].is_light())
                .collect(),
            state,
            offset: 0.0,
            initial_position: graph[node].global_position(),
            open_direction,
            open_offset_amount,
        }
    }

    pub fn resolve(&mut self, scene: &Scene) {
        self.initial_position = scene.graph[self.node].global_position();
    }

    fn set_lights_color(&self, graph: &mut Graph, color: Color) {
        for &light in self.lights.iter() {
            graph[light].as_light_mut().set_color(color);
        }
    }

    fn set_lights_enabled(&self, graph: &mut Graph, enabled: bool) {
        for &light in self.lights.iter() {
            graph[light].set_visibility(enabled);
        }
    }

    pub fn initial_position(&self) -> Vector3<f32> {
        self.initial_position
    }

    pub fn actual_position(&self, graph: &Graph) -> Vector3<f32> {
        let node_ref = &graph[self.node];
        node_ref.global_position()
    }

    pub fn node(&self) -> Handle<Node> {
        self.node
    }

    pub fn try_open(
        &mut self,
        sender: Sender<Message>,
        graph: &Graph,
        inventory: Option<&Inventory>,
    ) {
        let position = self.actual_position(graph);

        if self.state == DoorState::Closed {
            self.state = DoorState::Opening;

            sender
                .send(Message::PlaySound {
                    path: PathBuf::from("data/sounds/door_open.ogg"),
                    position,
                    gain: 0.6,
                    rolloff_factor: 1.0,
                    radius: 1.0,
                })
                .unwrap();
        } else if self.state == DoorState::Locked {
            let should_be_unlocked = inventory
                .map(|i| i.item_count(ItemKind::MasterKey) > 0)
                .unwrap_or(false);

            if should_be_unlocked {
                self.state = DoorState::Opening;

                sender
                    .send(Message::PlaySound {
                        path: PathBuf::from("data/sounds/door_open.ogg"),
                        position,
                        gain: 0.6,
                        rolloff_factor: 1.0,
                        radius: 1.0,
                    })
                    .unwrap();

                sender
                    .send(Message::PlaySound {
                        path: PathBuf::from("data/sounds/access_granted.ogg"),
                        position,
                        gain: 1.0,
                        rolloff_factor: 1.0,
                        radius: 1.0,
                    })
                    .unwrap();
            } else {
                sender
                    .send(Message::PlaySound {
                        path: PathBuf::from("data/sounds/door_deny.ogg"),
                        position,
                        gain: 1.0,
                        rolloff_factor: 1.0,
                        radius: 1.0,
                    })
                    .unwrap();
            }
        }
    }
}

#[derive(Default, Visit)]
pub struct DoorContainer {
    doors: Pool<Door>,
}

impl DoorContainer {
    pub fn new() -> Self {
        Self {
            doors: Default::default(),
        }
    }

    pub fn add(&mut self, door: Door) -> Handle<Door> {
        self.doors.spawn(door)
    }

    pub fn pair_iter(&self) -> impl Iterator<Item = (Handle<Door>, &Door)> {
        self.doors.pair_iter()
    }

    pub fn update(
        &mut self,
        actors: &ActorContainer,
        sender: Sender<Message>,
        scene: &mut Scene,
        dt: f32,
    ) {
        let speed = 0.55;

        for door in self.doors.iter_mut() {
            let node = &scene.graph[door.node];
            let move_direction = match door.open_direction {
                DoorDirection::Side => node.look_vector(),
                DoorDirection::Up => node.up_vector(),
            };

            let mut closest_actor = None;

            let someone_nearby = actors.iter().any(|a| {
                let actor_position = a.position(&scene.graph);
                // TODO: Replace with triggers.
                let close_enough = actor_position.metric_distance(&door.initial_position) < 1.25;
                if close_enough {
                    closest_actor = Some(a);
                }
                close_enough
            });

            if !someone_nearby && door.state == DoorState::Opened {
                door.state = DoorState::Closing;

                sender
                    .send(Message::PlaySound {
                        path: PathBuf::from("data/sounds/door_close.ogg"),
                        position: node.global_position(),
                        gain: 0.6,
                        rolloff_factor: 1.0,
                        radius: 1.0,
                    })
                    .unwrap();
            }

            match door.state {
                DoorState::Opening => {
                    if door.offset < door.open_offset_amount {
                        door.offset += speed * dt;
                        if door.offset >= door.open_offset_amount {
                            door.state = DoorState::Opened;
                            door.offset = door.open_offset_amount;
                        }
                    }

                    door.set_lights_enabled(&mut scene.graph, false);
                }
                DoorState::Closing => {
                    if door.offset > 0.0 {
                        door.offset -= speed * dt;
                        if door.offset <= 0.0 {
                            door.state = DoorState::Closed;
                            door.offset = 0.0;
                        }
                    }

                    door.set_lights_enabled(&mut scene.graph, false);
                }
                DoorState::Closed => {
                    door.set_lights_enabled(&mut scene.graph, true);
                    door.set_lights_color(&mut scene.graph, Color::opaque(0, 200, 0));
                }
                DoorState::Locked => {
                    door.set_lights_enabled(&mut scene.graph, true);
                    door.set_lights_color(&mut scene.graph, Color::opaque(200, 0, 0));
                }
                DoorState::Broken | DoorState::Opened => {
                    door.set_lights_enabled(&mut scene.graph, false);
                }
            };

            if let Some(body) = scene.physics_binder.body_of(door.node) {
                let body = scene.physics.bodies.get_mut(body).unwrap();
                body.set_position(
                    Isometry3 {
                        translation: Translation3 {
                            vector: door.initial_position
                                + move_direction
                                    .try_normalize(std::f32::EPSILON)
                                    .unwrap_or_default()
                                    .scale(door.offset),
                        },
                        rotation: body.position().rotation,
                    },
                    true,
                );
            }
        }
    }

    pub fn resolve(&mut self, scene: &Scene) {
        for door in self.doors.iter_mut() {
            door.resolve(scene)
        }
    }

    pub fn check_actor(
        &self,
        actor_position: Vector3<f32>,
        actor_handle: Handle<Actor>,
        sender: &Sender<Message>,
    ) {
        for (door_handle, door) in self.pair_iter() {
            let close_enough = actor_position.metric_distance(&door.initial_position()) < 1.25;
            if close_enough {
                sender
                    .send(Message::TryOpenDoor {
                        door: door_handle,
                        actor: actor_handle,
                    })
                    .unwrap();
            }
        }
    }
}

impl Index<Handle<Door>> for DoorContainer {
    type Output = Door;

    fn index(&self, index: Handle<Door>) -> &Self::Output {
        &self.doors[index]
    }
}

impl IndexMut<Handle<Door>> for DoorContainer {
    fn index_mut(&mut self, index: Handle<Door>) -> &mut Self::Output {
        &mut self.doors[index]
    }
}
