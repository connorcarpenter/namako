//! Shared test utilities and World types for namako integration tests.
//!
//! This module provides reusable World types following the DRY principle,
//! so individual test files don't need to define their own.

/// Mutable context wrapper for Given/When steps.
pub struct WorldMut<'a>(pub &'a mut World);

/// Immutable context wrapper for Then steps.
#[derive(Clone, Copy)]
pub struct WorldRef<'a>(pub &'a World);

impl<'a> WorldMut<'a> {
    pub fn new(world: &'a mut World) -> Self {
        Self(world)
    }
    pub fn world(&mut self) -> &mut World {
        self.0
    }
}

impl<'a> WorldRef<'a> {
    pub fn new(world: &'a World) -> Self {
        Self(world)
    }
    pub fn world(&self) -> &World {
        self.0
    }
}

/// A simple, stateless World for basic tests.
#[derive(Clone, Copy, Debug, Default, namako::World)]
#[world(mut_ctx = WorldMut<'a>, ref_ctx = WorldRef<'a>)]
pub struct World;
