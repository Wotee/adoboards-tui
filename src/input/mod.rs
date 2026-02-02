//! Input handling module
//!
//! This module contains all input-related functionality including:
//! - Key mapping and sequence matching
//! - Event handlers for different modes
//! - Main event loop

pub mod event_loop;
pub mod handlers;
pub mod keymap;

pub use event_loop::EventLoop;
