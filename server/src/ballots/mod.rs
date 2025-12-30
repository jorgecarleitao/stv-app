pub mod entity;
pub mod handlers;

pub use entity::{ActiveModel, Column, Entity, Model, Ranks};
pub use sea_orm::EntityTrait;

pub type Ballots = Entity;
