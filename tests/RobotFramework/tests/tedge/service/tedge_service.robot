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
    ${before}=    Get Service PID    dummy-service
    Execute Command    tedge service restart dummy-service
    ${after}=    Get Service PID    dummy-service
    Should Not Be Equal    ${before}    ${after}

Run tedge service for systemd service with a custom action
    Execute Command    tedge service reload dummy-service
    Execute Command    grep reloaded /tmp/dummy-service-reload.log

Run tedge service for service plugin
    Execute Command    tedge service restart name --service-type dummy
    # Confirm that the plugin was really executed
    File Should Exist    /tmp/dummy-service-plugin.log
    Execute Command    grep "restart name" /tmp/dummy-service-plugin.log
    # Unsupported action should return exit code 2
    Execute Command    tedge service invalid name --service-type dummy    exp_exit_code=2

tedge service does not support name and is_available
    ${stderr}=    Execute Command
    ...    tedge service name dummy-service    strip=True    exp_exit_code=2    stdout=False    stderr=True
    Should Contain    ${stderr}    'name' is not a service action
    ${stderr}=    Execute Command
    ...    tedge service is_available dummy-service    strip=True    exp_exit_code=2    stdout=False    stderr=True
    Should Contain    ${stderr}    'is_available' is not a service action

tedge service forwards stdout and stderr
    ${stdout}=    Execute Command    tedge service is_active dummy-service    strip=True
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
    Transfer To Device    ${CURDIR}/dummy-service.service    /etc/systemd/system/
    Execute Command    systemctl daemon-reload && systemctl start dummy-service
    Transfer To Device    ${CURDIR}/dummy    ${PLUGINS_DIR}/
    Execute Command    chmod +x ${PLUGINS_DIR}/dummy
