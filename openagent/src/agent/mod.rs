//! In-process agent module — the ReAct reasoning loop running inside openagent.
//!
//! Previously `services/agent/`; merged here so there is no separate binary
//! or TCP hop between openagent and the agent logic.
//!
//! Public surface used by the rest of openagent:
//!   - `handlers::AgentContext`  — shared context created once at startup
//!   - `handlers::handle_step()` — sync entry point called from AgentLayer / dispatch
//!   - `action::catalog::ActionCatalog` — discovers tools from service.json + skills
//!   - `tool_router::ToolRouter`         — dispatches tool calls to services over TCP

pub mod action;
pub mod classifier;
pub mod config;
pub mod core;
pub mod diary;
pub mod handlers;
pub mod llm;
pub mod memory_adapter;
pub mod metrics;
pub mod prompt;
pub mod tool_router;
