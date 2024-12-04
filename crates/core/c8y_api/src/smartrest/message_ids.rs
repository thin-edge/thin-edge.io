//! Definitions of Smartrest MQTT message ids.
//!
//! - https://cumulocity.com/docs/smartrest/smartrest-one/#built-in-messages
//! - https://cumulocity.com/docs/smartrest/mqtt-static-templates/

// Message IDs

pub const ERROR: usize = 41;
pub const JWT_TOKEN: usize = 71;

// Static templates

// Publish templates

// Inventory templates (1xx)

pub const DEVICE_CREATION: usize = 100;
pub const CHILD_DEVICE_CREATION: usize = 101;
pub const SERVICE_CREATION: usize = 102;
pub const SERVICE_STATUS_UPDATE: usize = 104;
pub const GET_CHILD_DEVICES: usize = 105;
pub const CLEAR_DEVICE_FRAGMENT: usize = 107;
pub const CONFIGURE_HARDWARE: usize = 110;
pub const CONFIGURE_MOBILE: usize = 111;
pub const CONFIGURE_POSITION: usize = 112;
pub const SET_CONFIGURATION: usize = 113;
pub const SET_SUPPORTED_OPERATIONS: usize = 114;
pub const SET_FIRMWARE: usize = 115;
pub const SET_SOFTWARE_LIST: usize = 116;
pub const SET_REQUIRED_AVAILABILITY: usize = 117;
pub const SET_SUPPORTED_LOGS: usize = 118;
pub const SET_SUPPORTED_CONFIGURATIONS: usize = 119;
pub const SET_CURRENTLY_INSTALLED_CONFIGURATION: usize = 120;
pub const SET_DEVICE_PROFILE_THAT_IS_BEING_APPLIED: usize = 121;
pub const SET_DEVICE_AGENT_INFORMATION: usize = 122;
pub const GET_DEVICE_MANAGED_OBJECT_ID: usize = 123;
pub const SEND_HEARTBEAT: usize = 125;
pub const SET_ADVANCED_SOFTWARE_LIST: usize = 140;
pub const APPEND_ADVANCED_SOFTWARE_ITEMS: usize = 141;
pub const REMOVE_ADVANCED_SOFTWARE_ITEMS: usize = 142;
pub const SET_SUPPORTED_SOFTWARE_TYPES: usize = 143;
pub const SET_CLOUD_REMOTE_ACCESS: usize = 150;

// Measurement templates (2xx)

pub const CREATE_CUSTOM_MEASUREMENT: usize = 200;
pub const CREATE_A_CUSTOM_MEASUREMENT_WITH_MULTIPLE_FRAGMENTS_AND_SERIES: usize = 201;
pub const CREATE_SIGNAL_STRENGTH_MEASUREMENT: usize = 210;
pub const CREATE_TEMPERATURE_MEASUREMENT: usize = 211;
pub const CREATE_BATTERY_MEASUREMENT: usize = 212;

// Alarm templates (3xx)

pub const CREATE_CRITICAL_ALARM: usize = 301;
pub const CREATE_MAJOR_ALARM: usize = 302;
pub const CREATE_MINOR_ALARM: usize = 303;
pub const CREATE_WARNING_ALARM: usize = 304;
pub const UPDATE_SEVERITY_OF_EXISTING_ALARM: usize = 305;
pub const CLEAR_EXISTING_ALARM: usize = 306;
pub const CLEAR_ALARM_FRAGMENT: usize = 307;

// Event templates (4xx)

pub const CREATE_BASIC_EVENT: usize = 400;
pub const CREATE_LOCATION_UPDATE_EVENT: usize = 401;
pub const CREATE_LOCATION_UPDATE_EVENT_WITH_DEVICE_UPDATE: usize = 402;
pub const CLEAR_EVENT_FRAGMENT: usize = 407;

// Operation templates (5xx)

pub const GET_PENDING_OPERATIONS: usize = 500;
pub const SET_OPERATION_TO_EXECUTING: usize = 501;
pub const SET_OPERATION_TO_FAILED: usize = 502;
pub const SET_OPERATION_TO_SUCCESSFUL: usize = 503;
pub const SET_OPERATION_TO_EXECUTING_ID: usize = 504;
pub const SET_OPERATION_TO_FAILED_ID: usize = 505;
pub const SET_OPERATION_TO_SUCCESSFUL_ID: usize = 506;
pub const SET_EXECUTING_OPERATIONS_TO_FAILED: usize = 507;

// Subscribe templates

// Inventory templates (1xx)

pub const GET_CHILDREN_OF_DEVICE: usize = 106;
pub const GET_DEVICE_MANAGED_OBJECT_ID_RESPONSE: usize = 124;

// Operation templates (5xx)

pub const RESTART: usize = 510;
pub const COMMAND: usize = 511;
pub const CONFIGURATION: usize = 513;
pub const FIRMWARE: usize = 515;
pub const SOFTWARE_LIST: usize = 516;
pub const MEASUREMENT_REQUEST_OPERATION: usize = 517;
pub const RELAY: usize = 518;
pub const RELAY_ARRAY: usize = 519;
pub const UPLOAD_CONFIGURATION_FILE: usize = 520;
pub const DOWNLOAD_CONFIGURATION_FILE: usize = 521;
pub const LOGFILE_REQUEST: usize = 522;
pub const COMMUNICATION_MODE: usize = 523;
pub const DOWNLOAD_CONFIGURATION_FILE_WITH_TYPE: usize = 524;
pub const FIRMWARE_FROM_PATCH: usize = 525;
pub const UPLOAD_CONFIGURATION_FILE_WITH_TYPE: usize = 526;
pub const SET_DEVICE_PROFILES: usize = 527;
pub const UPDATE_SOFTWARE: usize = 528;
pub const UPDATE_ADVANCED_SOFTWARE: usize = 529;
pub const CLOUD_REMOTE_ACCESS_CONNECT: usize = 530;
