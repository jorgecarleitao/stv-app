pub mod entity;
pub mod handlers;

pub use entity::{ActiveModel, Column, Entity, Model};
pub use sea_orm::EntityTrait;

pub type BallotTokens = Entity;
