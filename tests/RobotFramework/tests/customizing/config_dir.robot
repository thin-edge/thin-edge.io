*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:configuration
Suite Setup            Custom Suite Setup
Suite Teardown         Get Logs

*** Test Cases ***

thin-edge components support a custom config-dir location via flags
    ${CONFIG_DIR}=    Set Variable    /tmp/test_config_dir
    Create Config Dir    ${CONFIG_DIR}    ${DEVICE_SN}

    ThinEdgeIO.Directory Should Not Exist    /etc/tedge

    Should Not Contain Default Path    ${CONFIG_DIR}    tedge-mapper --config-dir ${CONFIG_DIR} c8y
    Should Not Contain Default Path    ${CONFIG_DIR}    tedge-agent --config-dir ${CONFIG_DIR}
    Should Not Contain Default Path    ${CONFIG_DIR}    c8y-firmware-plugin --config-dir ${CONFIG_DIR}

    Should Not Contain Default Path    ${CONFIG_DIR}    c8y-log-plugin --config-dir ${CONFIG_DIR}
    ThinEdgeIO.File Should Exist       ${CONFIG_DIR}/c8y/c8y-log-plugin.toml

    Should Not Contain Default Path    ${CONFIG_DIR}    c8y-configuration-plugin --config-dir ${CONFIG_DIR}
    ThinEdgeIO.File Should Exist       ${CONFIG_DIR}/c8y/c8y-configuration-plugin.toml

*** Keywords ***

Custom Suite Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-bootstrap --no-connect || true
    Stop Service    tedge-agent
    Stop Service    tedge-mapper-c8y
    Stop Service    c8y-configuration-plugin
    Stop Service    c8y-log-plugin
    Stop Service    c8y-firmware-plugin

    Set Suite Variable    ${DEVICE_SN}

Create Config Dir
    [Arguments]    ${CONFIG_DIR}    ${DEVICE_ID}
    Execute Command    rm -rf /etc/tedge
    Execute Command    rm -rf "${CONFIG_DIR}" && mkdir -p "${CONFIG_DIR}"
    Execute Command    tedge --config-dir "${CONFIG_DIR}" init

    # Set some default config so components will startup
    Execute Command    tedge --config-dir ${CONFIG_DIR} cert create --device-id "${DEVICE_ID}"
    Execute Command    tedge --config-dir ${CONFIG_DIR} config set c8y.url ${C8Y_CONFIG.host}

Should Not Contain Default Path
    [Documentation]    Check a thin-edge.io executable by running it for a short time (~5s), and 
    ...                analyzing the standard error output for any signs of the default
    ...                path /etc/tedge
    ...                This is only a rough check, as there is not a clean way to check
    ...                the current configuration of a specific component

    [Arguments]    ${CONFIG_DIR}    ${COMMAND}
    ${LOG_OUTPUT}=    Execute Command    timeout -s SIGKILL 5 ${COMMAND}    stderr=${True}    stdout=${False}    ignore_exit_code=${True}
    Should Not Contain    ${LOG_OUTPUT}    /etc/tedge
