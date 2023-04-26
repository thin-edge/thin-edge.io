*** Settings ***

Resource    ../../../resources/common.resource
Library     Cumulocity
Library     ThinEdgeIO
Library     DebugLibrary

Test Tags    theme:c8y    theme:monitoring    theme:mqtt
Test Setup    Custom Setup
Test Teardown    Get Logs


*** Variables ***
# Timeout in seconds used to wait for a service to be up. A longer timeout is
# sometimes needed in case if the service encounters a 'c8y_api::http_proxy: An
# error occurred while retrieving internal Id, operation will retry in 60
# seconds and mapper will reinitialize' error on startup, and then waits 60
# seconds before trying again. The timeout should be larger than this interval
# to give the service a chance to retry without failing the test
${TIMEOUT}=    ${90}

*** Test Cases ***

Test if all c8y services are up
    [Template]     Check if a service is up
    tedge-mapper-c8y
    tedge-agent
    c8y-configuration-plugin
    c8y-log-plugin
    c8y-firmware-plugin

Test if all c8y services are down
    [Template]     Check if a service is down
    tedge-mapper-c8y
    tedge-agent
    c8y-configuration-plugin
    c8y-log-plugin
    c8y-firmware-plugin


Test if all c8y services are using configured service type
    [Template]     Check if a service using configured service type
    tedge-mapper-c8y
    tedge-agent
    c8y-configuration-plugin
    c8y-log-plugin
    c8y-firmware-plugin

Test if all c8y services using default service type when service type configured as empty
    [Template]     Check if a service using configured service type as empty
    tedge-mapper-c8y
    tedge-agent
    c8y-configuration-plugin
    c8y-log-plugin
    c8y-firmware-plugin

Check health status of tedge-mapper-c8y service on broker stop start
    Custom Test Setup

    Device Should Exist                      ${DEVICE_SN}_tedge-mapper-c8y    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=up
    Should Be Equal    ${SERVICE["name"]}    tedge-mapper-c8y
    Should Be Equal    ${SERVICE["status"]}    up

    ThinEdgeIO.Stop Service    mosquitto.service
    ThinEdgeIO.Service Should Be Stopped  mosquitto.service

    Device Should Exist                      ${DEVICE_SN}_tedge-mapper-c8y    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=down
    Should Be Equal    ${SERVICE["name"]}    tedge-mapper-c8y
    Should Be Equal    ${SERVICE["status"]}    down

    ThinEdgeIO.Start Service    mosquitto.service
    ThinEdgeIO.Service Should Be Running  mosquitto.service

    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=up    timeout=${TIMEOUT}
    Should Be Equal    ${SERVICE["name"]}    tedge-mapper-c8y
    Should Be Equal    ${SERVICE["status"]}    up

    Custom Test Teardown

Check health status of tedge-mapper-c8y service on broker restart
    [Documentation]    Test tedge-mapper-c8y on mqtt broker restart
    Custom Test Setup

    Device Should Exist                      ${DEVICE_SN}_tedge-mapper-c8y    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=up    timeout=${TIMEOUT}
    Should Be Equal    ${SERVICE["name"]}    tedge-mapper-c8y
    Should Be Equal    ${SERVICE["status"]}    up

    ThinEdgeIO.Restart Service    mosquitto.service
    ThinEdgeIO.Service Should Be Running  mosquitto.service

    Sleep    5s    reason=Wait for any potential status changes to be sent to Cumulocity IoT
    Device Should Exist                      ${DEVICE_SN}_tedge-mapper-c8y    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=up    timeout=${TIMEOUT}
    Should Be Equal    ${SERVICE["name"]}    tedge-mapper-c8y
    Should Be Equal    ${SERVICE["status"]}    up

    Custom Test Teardown

Check health status of child device service
    [Documentation]    Test service status of child device services
    # Create the child device by sending the service status on tedge/health/<child-id>/<service-id
    # Verify if the service status is updated
    Set Device    ${DEVICE_SN}
    Set Suite Variable    $CHILD_SN    ${DEVICE_SN}_external-sensor
    Execute Command    tedge mqtt pub 'tedge/health/${CHILD_SN}/childservice' '{"type":"systemd","status":"unknown"}'

    Should Be A Child Device Of Device    ${CHILD_SN}

    # Check created service entries
    Device Should Exist                      ${DEVICE_SN}_${CHILD_SN}_childservice    show_info=False
    ${SERVICE}=    Device Should Have Fragment Values    status\=unknown
    Should Be Equal    ${SERVICE["name"]}    childservice
    Should Be Equal    ${SERVICE["serviceType"]}    systemd
    Should Be Equal    ${SERVICE["status"]}    unknown
    Should Be Equal    ${SERVICE["type"]}    c8y_Service


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}

Custom Test Setup
    ThinEdgeIO.Start Service    mosquitto
    ThinEdgeIO.Restart Service    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Running    tedge-mapper-c8y

Custom Test Teardown
    ThinEdgeIO.Stop Service    tedge-mapper-c8y
    ThinEdgeIO.Service Should Be Stopped    tedge-mapper-c8y

Check if a service is up
    [Arguments]    ${service_name}
    Custom Test Setup
    ThinEdgeIO.Start Service    ${service_name}
    ThinEdgeIO.Service Should Be Running    ${service_name}

    Device Should Exist                      ${DEVICE_SN}_${service_name}    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=up        timeout=${TIMEOUT}

    Should Be Equal    ${SERVICE["name"]}    ${service_name}
    Should Be Equal    ${SERVICE["serviceType"]}    service
    Should Be Equal    ${SERVICE["status"]}    up
    Should Be Equal    ${SERVICE["type"]}    c8y_Service
    ThinEdgeIO.Stop Service    ${service_name}
    Custom Test Teardown


Check if a service is down
    [Arguments]    ${service_name}
    Custom Test Setup
    ThinEdgeIO.Start Service    ${service_name}
    Device Should Exist                      ${DEVICE_SN}_${service_name}    show_info=False
    ThinEdgeIO.Stop Service    ${service_name}
    ThinEdgeIO.Service Should Be Stopped  ${service_name}

    Device Should Exist                      ${DEVICE_SN}_${service_name}    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=down

    Should Be Equal    ${SERVICE["name"]}    ${service_name}
    Should Be Equal    ${SERVICE["serviceType"]}    service
    Should Be Equal    ${SERVICE["status"]}    down
    Should Be Equal    ${SERVICE["type"]}    c8y_Service

    Custom Test Teardown

Check if a service using configured service type
    [Arguments]    ${service_name}
    Execute Command    tedge config set service.type thinedge
    Custom Test Setup
    ThinEdgeIO.Restart Service    ${service_name}
    Device Should Exist                      ${DEVICE_SN}_${service_name}    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=up    serviceType\=thinedge        timeout=${TIMEOUT}

    Should Be Equal    ${SERVICE["name"]}    ${service_name}
    Should Be Equal    ${SERVICE["serviceType"]}    thinedge
    Should Be Equal    ${SERVICE["status"]}    up
    Should Be Equal    ${SERVICE["type"]}    c8y_Service

    Custom Test Teardown

Check if a service using configured service type as empty
    [Arguments]    ${service_name}
    Execute Command    tedge config set service.type ""
    Custom Test Setup
    ThinEdgeIO.Restart Service    ${service_name}
    Device Should Exist                      ${DEVICE_SN}_${service_name}    show_info=False
    ${SERVICE}=    Cumulocity.Device Should Have Fragment Values    status\=up        serviceType\=service        timeout=${TIMEOUT}

    Should Be Equal    ${SERVICE["name"]}    ${service_name}
    Should Be Equal    ${SERVICE["serviceType"]}    service
    Should Be Equal    ${SERVICE["status"]}    up
    Should Be Equal    ${SERVICE["type"]}    c8y_Service

    Custom Test Teardown
