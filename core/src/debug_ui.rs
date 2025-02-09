mod avm1;
mod avm2;
mod display_object;
mod handle;
mod movie;

use crate::context::{RenderContext, UpdateContext};
use crate::debug_ui::avm1::Avm1ObjectWindow;
use crate::debug_ui::avm2::Avm2ObjectWindow;
use crate::debug_ui::display_object::DisplayObjectWindow;
use crate::debug_ui::handle::{AVM1ObjectHandle, AVM2ObjectHandle, DisplayObjectHandle};
use crate::debug_ui::movie::{MovieListWindow, MovieWindow};
use crate::display_object::TDisplayObject;
use crate::tag_utils::SwfMovie;
use gc_arena::DynamicRootSet;
use hashbrown::HashMap;
use ruffle_render::commands::CommandHandler;
use ruffle_render::matrix::Matrix;
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, Weak};
use swf::{Color, Rectangle, Twips};
use weak_table::PtrWeakKeyHashMap;

#[derive(Default)]
pub struct DebugUi {
    display_objects: HashMap<DisplayObjectHandle, DisplayObjectWindow>,
    movies: PtrWeakKeyHashMap<Weak<SwfMovie>, MovieWindow>,
    avm1_objects: HashMap<AVM1ObjectHandle, Avm1ObjectWindow>,
    avm2_objects: HashMap<AVM2ObjectHandle, Avm2ObjectWindow>,
    queued_messages: Vec<Message>,
    items_to_save: Vec<ItemToSave>,
    movie_list: Option<MovieListWindow>,
}

#[derive(Debug)]
pub enum Message {
    TrackDisplayObject(DisplayObjectHandle),
    TrackMovie(Arc<SwfMovie>),
    TrackAVM1Object(AVM1ObjectHandle),
    TrackAVM2Object(AVM2ObjectHandle),
    TrackStage,
    TrackTopLevelMovie,
    ShowKnownMovies,
    SaveFile(ItemToSave),
}

impl DebugUi {
    pub(crate) fn show(&mut self, egui_ctx: &egui::Context, context: &mut UpdateContext) {
        let mut messages = std::mem::take(&mut self.queued_messages);

        self.display_objects.retain(|object, window| {
            let object = object.fetch(context.dynamic_root);
            window.show(egui_ctx, context, object, &mut messages)
        });

        self.avm1_objects.retain(|object, window| {
            let object = object.fetch(context.dynamic_root);
            window.show(egui_ctx, context, object, &mut messages)
        });

        self.avm2_objects.retain(|object, window| {
            let object = object.fetch(context.dynamic_root);
            window.show(egui_ctx, context, object, &mut messages)
        });

        self.movies
            .retain(|movie, window| window.show(egui_ctx, context, movie, &mut messages));

        if let Some(mut movie_list) = self.movie_list.take() {
            if movie_list.show(egui_ctx, context, &mut messages) {
                self.movie_list = Some(movie_list);
            }
        }

        for message in messages {
            match message {
                Message::TrackDisplayObject(object) => {
                    self.track_display_object(object);
                }
                Message::TrackStage => {
                    self.track_display_object(DisplayObjectHandle::new(context, context.stage));
                }
                Message::TrackMovie(movie) => {
                    self.movies.insert(movie, Default::default());
                }
                Message::TrackTopLevelMovie => {
                    self.movies.insert(context.swf.clone(), Default::default());
                }
                Message::TrackAVM1Object(object) => {
                    self.avm1_objects.insert(object, Default::default());
                }
                Message::TrackAVM2Object(object) => {
                    self.avm2_objects.insert(object, Default::default());
                }
                Message::SaveFile(file) => {
                    self.items_to_save.push(file);
                }
                Message::ShowKnownMovies => {
                    self.movie_list = Some(Default::default());
                }
            }
        }
    }

    pub fn items_to_save(&mut self) -> Vec<ItemToSave> {
        std::mem::take(&mut self.items_to_save)
    }

    pub fn queue_message(&mut self, message: Message) {
        self.queued_messages.push(message);
    }

    pub fn track_display_object(&mut self, handle: DisplayObjectHandle) {
        self.display_objects.insert(handle, Default::default());
    }

    pub fn draw_debug_rects<'gc>(
        &self,
        context: &mut RenderContext<'_, 'gc>,
        dynamic_root_set: DynamicRootSet<'gc>,
    ) {
        let world_matrix = context.stage.view_matrix() * *context.stage.base().matrix();

        for (object, window) in self.display_objects.iter() {
            if let Some(color) = window.debug_rect_color() {
                let object = object.fetch(dynamic_root_set);
                let bounds = world_matrix * object.world_bounds();

                draw_debug_rect(context, color, bounds, 3.0);
            }

            if let Some(object) = window.hovered_debug_rect() {
                let object = object.fetch(dynamic_root_set);
                let bounds = world_matrix * object.world_bounds();

                draw_debug_rect(context, swf::Color::RED, bounds, 5.0);
            }
        }

        for (_object, window) in self.avm1_objects.iter() {
            if let Some(object) = window.hovered_debug_rect() {
                let object = object.fetch(dynamic_root_set);
                let bounds = world_matrix * object.world_bounds();

                draw_debug_rect(context, swf::Color::RED, bounds, 5.0);
            }
        }

        for (_object, window) in self.avm2_objects.iter() {
            if let Some(object) = window.hovered_debug_rect() {
                let object = object.fetch(dynamic_root_set);
                let bounds = world_matrix * object.world_bounds();

                draw_debug_rect(context, swf::Color::RED, bounds, 5.0);
            }
        }
    }
}

pub struct ItemToSave {
    pub suggested_name: String,
    pub data: Vec<u8>,
}

impl Debug for ItemToSave {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemToSave")
            .field("suggested_name", &self.suggested_name)
            .field("data", &self.data.len())
            .finish()
    }
}

fn draw_debug_rect(
    context: &mut RenderContext,
    color: Color,
    bounds: Rectangle<Twips>,
    thickness: f32,
) {
    let width = bounds.width().to_pixels() as f32;
    let height = bounds.height().to_pixels() as f32;
    let thickness_twips = Twips::from_pixels(thickness as f64);

    // Top
    context.commands.draw_rect(
        color.clone(),
        Matrix::create_box(
            width,
            thickness,
            0.0,
            bounds.x_min,
            bounds.y_min - thickness_twips,
        ),
    );
    // Bottom
    context.commands.draw_rect(
        color.clone(),
        Matrix::create_box(width, thickness, 0.0, bounds.x_min, bounds.y_max),
    );
    // Left
    context.commands.draw_rect(
        color.clone(),
        Matrix::create_box(
            thickness,
            height,
            0.0,
            bounds.x_min - thickness_twips,
            bounds.y_min,
        ),
    );
    // Right
    context.commands.draw_rect(
        color,
        Matrix::create_box(thickness, height, 0.0, bounds.x_max, bounds.y_min),
    );
}
