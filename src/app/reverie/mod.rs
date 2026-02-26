//! Reverie — background sub-agents that share the main worker's context.
//!
//! A Reverie runs in the same event loop as the main agent, with its own
//! LLM stream and conversation, but sharing all panels and state.
//! The first reverie type is the **Context Optimizer**, which reshapes
//! context for relevance and budget.

pub mod streaming;
pub mod tools;
pub mod trigger;
