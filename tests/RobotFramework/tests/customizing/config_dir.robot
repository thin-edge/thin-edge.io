*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Suite Setup
Suite Teardown      Get Logs

Test Tags           theme:cli    theme:configuration


*** Test Cases ***
thin-edge components support a custom config-dir location via flags
    ${CONFIG_DIR}=    Set Variable    /tmp/test_config_dir
    Create Config Dir    ${CONFIG_DIR}    ${DEVICE_SN}

    ThinEdgeIO.Directory Should Not Exist    /etc/tedge

    Should Not Contain Default Path    tedge-mapper --config-dir ${CONFIG_DIR} c8y
    Should Not Contain Default Path    tedge-agent --config-dir ${CONFIG_DIR}
    Should Not Contain Default Path    c8y-firmware-plugin --config-dir ${CONFIG_DIR}

    Should Not Contain Default Path    tedge-log-plugin --config-dir ${CONFIG_DIR}
    ThinEdgeIO.File Should Exist    ${CONFIG_DIR}/plugins/tedge-log-plugin.toml

    Should Not Contain Default Path    tedge-configuration-plugin --config-dir ${CONFIG_DIR}
    ThinEdgeIO.File Should Exist    ${CONFIG_DIR}/plugins/tedge-configuration-plugin.toml

thin-edge components support a custom config-dir location via an environment variable
    ${CONFIG_DIR}=    Set Variable    /tmp/test_config_dir_from_env
    Create Config Dir    ${CONFIG_DIR}    ${DEVICE_SN}

    ThinEdgeIO.Directory Should Not Exist    /etc/tedge

    Should Not Contain Default Path    tedge-mapper c8y    env=TEDGE_CONFIG_DIR=${CONFIG_DIR}
    Should Not Contain Default Path    tedge-agent    env=TEDGE_CONFIG_DIR=${CONFIG_DIR}
    Should Not Contain Default Path    c8y-firmware-plugin    env=TEDGE_CONFIG_DIR=${CONFIG_DIR}

    Should Not Contain Default Path    tedge-log-plugin    env=TEDGE_CONFIG_DIR=${CONFIG_DIR}
    ThinEdgeIO.File Should Exist    ${CONFIG_DIR}/plugins/tedge-log-plugin.toml

    Should Not Contain Default Path
    ...    tedge-configuration-plugin
    ...    env=TEDGE_CONFIG_DIR=${CONFIG_DIR}
    ThinEdgeIO.File Should Exist    ${CONFIG_DIR}/plugins/tedge-configuration-plugin.toml

Ignore env variable by using an empty value
    [Documentation]    If the TEDGE_CONFIG_DIR setting has been set globally, then
    ...    sometimes it is still useful to be able to override the value without having to unset it.
    ...    A common way to do this is just to give an empty value to TEDGE_CONFIG_DIR before
    ...    launching a binary
    ${CONFIG_DIR}=    Set Variable    /tmp/test_config_dir_reset
    Create Config Dir    ${CONFIG_DIR}    ${DEVICE_SN}

    Execute Command    cmd=TEDGE_CONFIG_DIR=${CONFIG_DIR} sh -c 'tedge init'
    ThinEdgeIO.Directory Should Exist    ${CONFIG_DIR}
    ThinEdgeIO.Directory Should Not Exist    /etc/tedge

    # Ignore the globally set TEDGE_CONFIG_DIR value (for the process), but
    # setting an empty value in the shell, e.g. TEDGE_CONFIG_DIR=
    Execute Command    cmd=TEDGE_CONFIG_DIR=${CONFIG_DIR} sh -c 'TEDGE_CONFIG_DIR= tedge init'
    ThinEdgeIO.Directory Should Exist    /etc/tedge


*** Keywords ***
Custom Suite Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-bootstrap --no-connect || true
    Stop Service    tedge-agent
    Stop Service    tedge-mapper-c8y
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
    ...    analyzing the standard error output for any signs of the default
    ...    path /etc/tedge
    ...    This is only a rough check, as there is not a clean way to check
    ...    the current configuration of a specific component
    [Arguments]    ${COMMAND}    ${env}=

    ${LOG_OUTPUT}=    Execute Command
    ...    ${env} timeout -s SIGKILL 5 ${COMMAND}
    ...    stderr=${True}
    ...    stdout=${False}
    ...    ignore_exit_code=${True}

    Should Not Contain    ${LOG_OUTPUT}    /etc/tedge
