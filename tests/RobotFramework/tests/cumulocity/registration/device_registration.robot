*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:registration
Suite Setup    Custom Setup
Test Setup    Test Setup
Test Teardown    Get Logs    ${DEVICE_SN}

*** Test Cases ***

Main device registration
    ${mo}=    Device Should Exist              ${DEVICE_SN}
    ${mo}=    Cumulocity.Device Should Have Fragment Values    name\=${DEVICE_SN}
    Should Be Equal    ${mo["owner"]}    device_${DEVICE_SN}
    Should Be Equal    ${mo["name"]}    ${DEVICE_SN}


Child device registration
    Execute Command    mkdir -p /etc/tedge/operations/c8y/${CHILD_ID}
    Restart Service    tedge-mapper-c8y

    # Check registration
    ${child_mo}=    Device Should Exist        ${CHILD_SN}
    ${child_mo}=    Cumulocity.Device Should Have Fragment Values    name\=${CHILD_ID}
    Should Be Equal    ${child_mo["owner"]}    device_${DEVICE_SN}    # The parent is the owner of the child
    Should Be Equal    ${child_mo["name"]}     ${CHILD_ID}

    # Check child device relationship
    Cumulocity.Set Device    ${DEVICE_SN}
    Cumulocity.Should Be A Child Device Of Device    ${CHILD_SN}

Register child device with defaults via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_ID}//' '{"@type":"child-device"}'
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_SN}    child_name=${CHILD_SN}    child_type=thin-edge.io-child

Register child device with custom name and type via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_ID}//' '{"@type":"child-device","name":"${CHILD_ID}","type":"linux-device-Aböut"}'
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_SN}    child_name=${CHILD_ID}    child_type=linux-device-Aböut

Register child device with custom id via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_ID}//' '{"@type":"child-device","@id":"custom-${CHILD_SN}","name":"custom-${CHILD_ID}"}'
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=custom-${CHILD_SN}    child_name=custom-${CHILD_ID}    child_type=thin-edge.io-child

Register nested child device using default topic schema via MQTT
    ${child_level1}=    Get Random Name
    ${child_level2}=    Get Random Name
    ${child_level3}=    Get Random Name

    Execute Command    tedge mqtt pub --retain 'te/device/${child_level1}//' '{"@type":"child-device","@parent":"device/main//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${child_level2}//' '{"@type":"child-device","@parent":"device/${child_level1}//","name":"${child_level2}"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${child_level3}//' '{"@type":"child-device","@parent":"device/${child_level2}//","type":"child_level3"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${child_level3}/service/custom-app' '{"@type":"service","@parent":"device/${child_level3}//","name":"custom-app","type":"service-level3"}'

    # Level 1
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${DEVICE_SN}:device:${child_level1}    child_name=${DEVICE_SN}:device:${child_level1}    child_type=thin-edge.io-child

    # Level 2
    Check Child Device    parent_sn=${DEVICE_SN}:device:${child_level1}    child_sn=${DEVICE_SN}:device:${child_level2}    child_name=${child_level2}    child_type=thin-edge.io-child

    # Level 3
    Check Child Device    parent_sn=${DEVICE_SN}:device:${child_level2}    child_sn=${DEVICE_SN}:device:${child_level3}    child_name=${DEVICE_SN}:device:${child_level3}    child_type=child_level3
    Check Service    child_sn=${DEVICE_SN}:device:${child_level3}    service_sn=${DEVICE_SN}:device:${child_level3}:service:custom-app    service_name=custom-app    service_type=service-level3    service_status=up


Register service on a child device via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_ID}//' '{"@type":"child-device","name":"${CHILD_ID}","type":"linux-device-Aböut"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_ID}/service/custom-app' '{"@type":"service","@parent":"device/${CHILD_ID}//","name":"custom-app","type":"custom-type"}'

    # Check child registration
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_SN}    child_name=${CHILD_ID}    child_type=linux-device-Aböut

    # Check service registration
    Check Service    child_sn=${CHILD_SN}    service_sn=${CHILD_SN}:service:custom-app    service_name=custom-app    service_type=custom-type    service_status=up


Register devices using custom MQTT schema
    [Documentation]    Complex example showing how to use custom MQTT topics to register devices/services using
        ...            custom identity schemas

    Execute Command    tedge mqtt pub --retain 'te/base///' '{"@type":"device","name":"base","type":"te_gateway"}'

    Execute Command    tedge mqtt pub --retain 'te/factory1/shop1/plc1/sensor1' '{"@type":"child-device","name":"sensor1","type":"SmartSensor"}'
    Execute Command    tedge mqtt pub --retain 'te/factory1/shop1/plc1/sensor2' '{"@type":"child-device","name":"sensor2","type":"SmartSensor"}'

    # Service of main device
    Execute Command    tedge mqtt pub --retain 'te/factory1/shop1/plc1/metrics' '{"@type":"service","name":"metrics","type":"PLCApplication"}'

    # Service of child device
    Execute Command    tedge mqtt pub --retain 'te/factory1/shop1/apps/sensor1' '{"@type":"service","@parent":"factory1/shop1/plc1/sensor1","name":"metrics","type":"PLCMonitorApplication"}'
    Execute Command    tedge mqtt pub --retain 'te/factory1/shop1/apps/sensor2' '{"@type":"service","@parent":"factory1/shop1/plc1/sensor2","name":"metrics","type":"PLCMonitorApplication"}'

    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${DEVICE_SN}:factory1:shop1:plc1:sensor1    child_name=sensor1    child_type=SmartSensor
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${DEVICE_SN}:factory1:shop1:plc1:sensor2    child_name=sensor2    child_type=SmartSensor

    # Check main device services
    Cumulocity.Set Device    ${DEVICE_SN}
    Should Have Services    name=metrics    service_type=PLCApplication    status=up

    # Check child services
    Cumulocity.Set Device    ${DEVICE_SN}:factory1:shop1:plc1:sensor1
    Should Have Services    name=metrics    service_type=PLCMonitorApplication    status=up

    Cumulocity.Set Device    ${DEVICE_SN}:factory1:shop1:plc1:sensor2
    Should Have Services    name=metrics    service_type=PLCMonitorApplication    status=up

    # Publish to main device on custom topic
    Execute Command    cmd=tedge mqtt pub te/base////m/gateway_stats '{"runtime":1001}'
    Cumulocity.Set Device    ${DEVICE_SN}
    Cumulocity.Device Should Have Measurements    type=gateway_stats    minimum=1    maximum=1

*** Keywords ***

Check Child Device
    [Arguments]    ${parent_sn}    ${child_sn}    ${child_name}    ${child_type}
    ${child_mo}=    Device Should Exist        ${child_sn}

    ${child_mo}=    Cumulocity.Device Should Have Fragment Values    name\=${child_name}
    Should Be Equal    ${child_mo["owner"]}    device_${DEVICE_SN}    # The parent is the owner of the child
    Should Be Equal    ${child_mo["name"]}     ${child_name}
    Should Be Equal    ${child_mo["type"]}     ${child_type}

    # Check child device relationship
    Cumulocity.Device Should Exist    ${parent_sn}
    Cumulocity.Should Be A Child Device Of Device    ${child_sn}

Check Service
    [Arguments]    ${child_sn}    ${service_sn}    ${service_name}    ${service_type}    ${service_status}=up
    Cumulocity.Device Should Exist    ${service_sn}    show_info=${False}
    Cumulocity.Device Should Exist    ${child_sn}    show_info=${False}
    Should Have Services    name=${service_name}    service_type=${service_type}    status=${service_status}


Test Setup
    ${CHILD_ID}=    Get Random Name
    Set Test Variable    $CHILD_ID
    Set Test Variable    $CHILD_SN    ${DEVICE_SN}:device:${CHILD_ID}

    ThinEdgeIO.Set Device Context    ${DEVICE_SN}

Custom Setup
    ${DEVICE_SN}=                    Setup
    Set Suite Variable               $DEVICE_SN
