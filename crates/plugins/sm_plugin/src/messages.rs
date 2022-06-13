use tedge_actors::message_type;
use tedge_actors::{Message, Recipient};

// Software Management Requests
message_type!(SMRequest[ListSoftwareModules,UpdateSoftwareModules,UpdateSoftwareModule]);
message_type!(SMManagerRequest[RegisterSoftwareModule,ListSoftwareModules,UpdateSoftwareModules,UpdateSoftwareModule]);

// Software Management Responses
message_type!(SMResponse[SoftwareModuleList,SoftwareModulesUpdates,SoftwareModulesUpdate]);
message_type!(SMManagerResponse[SoftwareModuleRegistration,SoftwareModuleList,SoftwareModulesUpdates,SoftwareModulesUpdate]);

/// Register a software module actor for a type
#[derive(Debug)]
pub struct RegisterSoftwareModule {
    pub module_type: SoftwareType,
    pub actor: Recipient<SMRequest>,
}

impl Clone for RegisterSoftwareModule {
    fn clone(&self) -> Self {
        RegisterSoftwareModule {
            module_type: self.module_type.clone(),
            actor: self.actor.clone(),
        }
    }
}

/// Outcome of a software module registration
pub struct SoftwareModuleRegistration {
    pub module_type: SoftwareType,
}

/// Request the current list of installed software modules
#[derive(Clone, Debug)]
pub struct ListSoftwareModules;

impl Message for ListSoftwareModules {}

/// A list of installed software modules
#[derive(Clone, Debug)]
pub struct SoftwareModuleList {
    pub modules: Vec<SoftwareModule>,
}

impl Message for SoftwareModuleList {}

/// Request to update a set of software modules
#[derive(Clone, Debug)]
pub struct UpdateSoftwareModules {
    pub updates: Vec<UpdateSoftwareModule>,
}

impl Message for UpdateSoftwareModules {}

/// Request to update a set of software modules
#[derive(Clone, Debug)]
pub struct SoftwareModulesUpdates {
    pub results: Vec<SoftwareModulesUpdate>,
}

impl Message for SoftwareModulesUpdates {}

/// Request to update a given software module
#[derive(Clone, Debug)]
pub enum UpdateSoftwareModule {
    Install(SoftwareModule),
    Remove(SoftwareModule),
}

impl Message for UpdateSoftwareModule {}

/// Outcome of an update request
#[derive(Clone, Debug)]
pub struct SoftwareModulesUpdate {
    pub update: UpdateSoftwareModule,
    pub result: Result<(), String>,
}

impl Message for SoftwareModulesUpdate {}

/// Definition of a software module
#[derive(Clone, Debug)]
pub struct SoftwareModule {
    pub module_type: SoftwareType,
    pub module_name: SoftwareName,
    pub module_version: Option<SoftwareVersion>,
}

pub type SoftwareType = String;
pub type SoftwareName = String;
pub type SoftwareVersion = String;
