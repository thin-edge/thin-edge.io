*** Settings ***
Library             ThinEdgeIO

Suite Setup         Custom Suite Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:services    theme:cli    theme:plugins


*** Variables ***
${PLUGINS_DIR}      /usr/share/tedge/service-plugins


*** Test Cases ***
Service plugin directory is created by package installation
    Directory Should Exist    ${PLUGINS_DIR}
    Path Should Have Permissions    ${PLUGINS_DIR}    755

Run tedge service for systemd service
    ${before}=    Get Service PID    sleeper
    Execute Command    tedge service restart sleeper
    ${after}=    Get Service PID    sleeper
    Should Not Be Equal    ${before}    ${after}

Run tedge service for systemd service with a custom action
    Execute Command    tedge service reload sleeper
    Execute Command    grep reloaded /tmp/sleeper-reload.log

Run tedge service for service plugin
    Execute Command    tedge service restart name --type dummy
    # Confirm that the plugin was really executed
    File Should Exist    /tmp/dummy-plugin.log
    Execute Command    grep "restart name" /tmp/dummy-plugin.log
    # Unsupported action should return exit code 2
    Execute Command    tedge service invalid name --type dummy    exp_exit_code=2

Run tedge service for a service type with no plugin
    Execute Command    tedge service restart name --type no_such_backend    exp_exit_code=2

tedge service terminates an action not completing in time
    ${stderr}=    Execute Command
    ...    tedge service hang name --type dummy --timeout 1s
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=124
    Should Contain    ${stderr}    did not complete within 1s
    Execute Command    pgrep -f 'sleep 123[4]'    exp_exit_code=1

tedge service terminates the action when it is stopped
    Execute Command    nohup tedge service hang name --type dummy >/dev/null 2>&1 &
    Wait Until Keyword Succeeds    10x    1s    Execute Command    pgrep -f 'sleep 123[4]'
    Execute Command    pkill -TERM -f 'tedge service han[g]'
    Wait Until Keyword Succeeds    10x    1s    Execute Command    pgrep -f 'sleep 123[4]'    exp_exit_code=1

tedge service does not support name and is_available
    ${stderr}=    Execute Command
    ...    tedge service name sleeper    strip=True    exp_exit_code=2    stdout=False    stderr=True
    Should Contain    ${stderr}    'name' is a reserved key in system.toml
    ${stderr}=    Execute Command
    ...    tedge service is_available sleeper    strip=True    exp_exit_code=2    stdout=False    stderr=True
    Should Contain    ${stderr}    'is_available' is a reserved key in system.toml

tedge service forwards stdout and stderr
    ${stdout}=    Execute Command    tedge service is_active sleeper    strip=True
    Should Contain    ${stdout}    active
    ${stdout}=    Execute Command    tedge service is_active tedge-mapper-c8y    strip=True    exp_exit_code=1
    Should Contain    ${stdout}    inactive
    ${stderr}=    Execute Command
    ...    tedge service reload tedge-mapper-c8y
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    exp_exit_code=1
    ${systemd}=    Execute Command
    ...    systemctl reload tedge-mapper-c8y
    ...    strip=True
    ...    stdout=False
    ...    stderr=True
    ...    ignore_exit_code=True
    Should Contain    ${stderr}    ${systemd}


*** Keywords ***
Custom Suite Setup
    Setup    register=${False}
    Transfer To Device    ${CURDIR}/system.toml    /etc/tedge/system.toml
    Transfer To Device    ${CURDIR}/sleeper.service    /etc/systemd/system/
    Execute Command    systemctl daemon-reload && systemctl start sleeper
    Transfer To Device    ${CURDIR}/dummy    ${PLUGINS_DIR}/
    Execute Command    chmod +x ${PLUGINS_DIR}/dummy
