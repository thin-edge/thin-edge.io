//! SystemCommand runner facility.

mod core;
mod system_command_runner;
mod system_commands;

pub trait AbstractSystemCommandRunner:
    RunSystemCommand<SystemdStopService>
    + RunSystemCommand<SystemdRestartService>
    + RunSystemCommand<SystemdEnableService>
    + RunSystemCommand<SystemdDisableService>
    + RunSystemCommand<SystemdIsServiceActive>
    + RunSystemCommand<SystemdVersion>
    + std::fmt::Debug
{
}

pub use self::{core::*, system_command_runner::SystemCommandRunner, system_commands::*};
