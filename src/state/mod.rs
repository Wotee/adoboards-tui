//! State management module
//!
//! This module contains all application state management, separated into:
//! - board_state: Core business logic state (work items, sources, caches)
//! - ui_state: UI-specific state (in ../ui_state.rs)

pub mod board_state;

pub use board_state::BoardState;
