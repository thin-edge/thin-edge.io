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
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"${CHILD_SN}"}'

    # Check registration
    ${child_mo}=    Device Should Exist        ${CHILD_SN}
    ${child_mo}=    Cumulocity.Device Should Have Fragment Values    name\=${CHILD_SN}
    Should Be Equal    ${child_mo["owner"]}    device_${DEVICE_SN}    # The parent is the owner of the child
    Should Be Equal    ${child_mo["name"]}     ${CHILD_SN}

    # Check child device relationship
    Cumulocity.Set Device    ${DEVICE_SN}
    Cumulocity.Should Be A Child Device Of Device    ${CHILD_SN}

    #Deregister Child device
    Run Keyword And Ignore Error    Execute Command    mosquitto_sub --remove-retained -W 3 -t 'te/device/${CHILD_SN}//'
    

Register child device with defaults via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device"}'
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_XID}    child_name=${CHILD_XID}    child_type=thin-edge.io-child

    #Deregister Child device
    Run Keyword And Ignore Error    Execute Command    mosquitto_sub --remove-retained -W 3 -t 'te/device/${CHILD_SN}//'

Register service on a child device via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","name":"${CHILD_SN}","type":"linux-device-Aböut"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/custom-app' '{"@type":"service","@parent":"device/${CHILD_SN}//","name":"custom-app","type":"custom-type"}'

    # Check child registration
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_XID}    child_name=${CHILD_SN}    child_type=linux-device-Aböut

    # Check service registration
    Check Service    child_sn=${CHILD_XID}    service_sn=${CHILD_XID}:service:custom-app    service_name=custom-app    service_type=custom-type    service_status=up

    #Deregister service on a child device
    Run Keyword And Ignore Error    Execute Command    mosquitto_sub --remove-retained -W 3 -t 'te/device/${CHILD_SN}/service/custom-app/#'





*** Keywords ***

Re-enable auto-registration and collect logs
    [Teardown]    Get Logs    ${DEVICE_SN}
    Execute Command    sudo tedge config unset c8y.entity_store.auto_register
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

Enable clean start and collect logs
    [Teardown]    Get Logs    ${DEVICE_SN}
    Execute Command    sudo tedge config set c8y.entity_store.clean_start true
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

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
    ${CHILD_SN}=    Get Random Name
    Set Test Variable    $CHILD_SN
    Set Test Variable    $CHILD_XID    ${DEVICE_SN}:device:${CHILD_SN}

    ThinEdgeIO.Set Device Context    ${DEVICE_SN}

Custom Setup
    ${DEVICE_SN}=                    Setup
    Set Suite Variable               $DEVICE_SN
