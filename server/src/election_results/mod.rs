pub mod entity;

pub use entity::{ActiveModel, Column, Entity, Model};
pub use sea_orm::EntityTrait;

pub type ElectionResults = Entity;
