*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:registration    theme:deregistration
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

    # Deregister Child device
    Execute Command    mosquitto_sub --remove-retained -W 3 -t "te/device/${CHILD_SN}/+/+/#"    exp_exit_code=27
    
    # Check if deregistration was successful
    Sleep    1s    reason=Allowing components to process messages
    Should Have Retained Message Count    te/device/${CHILD_SN}/+/+/#    0     

    # Checking if child device will be recreated after mapper restart
    Restart Service    tedge-mapper-c8y    
    Sleep    5s    reason=Allowing startup to complete
    Should Have Retained Message Count    te/device/${CHILD_SN}/+/+/#    0  

Register child device with defaults via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device"}'
    Should Have MQTT Messages    te/device/${CHILD_SN}//    message_contains="@id":"${CHILD_XID}"    message_contains="@type":"child-device"
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_XID}    child_name=${CHILD_XID}    child_type=thin-edge.io-child

Register child device with custom name and type via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","name":"${CHILD_SN}","type":"linux-device-Aböut"}'
    Should Have MQTT Messages    te/device/${CHILD_SN}//    message_contains="@id":"${CHILD_XID}"    message_contains="@type":"child-device"
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_XID}    child_name=${CHILD_SN}    child_type=linux-device-Aböut

Register child device with custom id via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","@id":"custom-${CHILD_XID}","name":"custom-${CHILD_SN}"}'
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=custom-${CHILD_XID}    child_name=custom-${CHILD_SN}    child_type=thin-edge.io-child

Register nested child device using default topic schema via MQTT
    ${child_level1}=    Get Random Name
    ${child_level2}=    Get Random Name
    ${child_level3}=    Get Random Name

    Execute Command    tedge mqtt pub --retain 'te/device/${child_level1}//' '{"@type":"child-device","@parent":"device/main//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${child_level2}//' '{"@type":"child-device","@parent":"device/${child_level1}//","name":"${child_level2}"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${child_level3}//' '{"@type":"child-device","@parent":"device/${child_level2}//","type":"child_level3"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${child_level3}/service/custom-app' '{"@type":"service","@parent":"device/${child_level3}//","name":"custom-app","type":"service-level3"}'

    # Level 1
    Should Have MQTT Messages    te/device/${child_level1}//    message_contains="@id":"${DEVICE_SN}:device:${child_level1}"    message_contains="@type":"child-device"
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${DEVICE_SN}:device:${child_level1}    child_name=${DEVICE_SN}:device:${child_level1}    child_type=thin-edge.io-child

    # Level 2
    Should Have MQTT Messages    te/device/${child_level2}//    message_contains="@id":"${DEVICE_SN}:device:${child_level2}"    message_contains="@type":"child-device"
    Check Child Device    parent_sn=${DEVICE_SN}:device:${child_level1}    child_sn=${DEVICE_SN}:device:${child_level2}    child_name=${child_level2}    child_type=thin-edge.io-child

    # Level 3
    Should Have MQTT Messages    te/device/${child_level3}//    message_contains="@id":"${DEVICE_SN}:device:${child_level3}"    message_contains="@type":"child-device"
    Check Child Device    parent_sn=${DEVICE_SN}:device:${child_level2}    child_sn=${DEVICE_SN}:device:${child_level3}    child_name=${DEVICE_SN}:device:${child_level3}    child_type=child_level3
    Should Have MQTT Messages    te/device/${child_level3}/service/custom-app    message_contains="@id":"${DEVICE_SN}:device:${child_level3}:service:custom-app"    message_contains="@type":"service"
    Check Service    child_sn=${DEVICE_SN}:device:${child_level3}    service_sn=${DEVICE_SN}:device:${child_level3}:service:custom-app    service_name=custom-app    service_type=service-level3    service_status=up


Register service on a child device via MQTT
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}//' '{"@type":"child-device","name":"${CHILD_SN}","type":"linux-device-Aböut"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${CHILD_SN}/service/custom-app' '{"@type":"service","@parent":"device/${CHILD_SN}//","name":"custom-app","type":"custom-type"}'

    # Check child registration
    Check Child Device    parent_sn=${DEVICE_SN}    child_sn=${CHILD_XID}    child_name=${CHILD_SN}    child_type=linux-device-Aböut

    # Check service registration
    Check Service    child_sn=${CHILD_XID}    service_sn=${CHILD_XID}:service:custom-app    service_name=custom-app    service_type=custom-type    service_status=up

    # Deregister service on a child device
    Execute Command    mosquitto_sub --remove-retained -W 3 -t 'te/device/${CHILD_SN}/service/custom-app/#'    exp_exit_code=27

    # Check if deregistration was successful
    Sleep    1s    reason=Allowing components to process messages
    Should Have Retained Message Count    te/device/${CHILD_SN}/service/custom-app/#    0   

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


    # Check if MQTT device/service representations contains @id
    Should Have MQTT Messages    te/base///    message_contains="@id":"${DEVICE_SN}"    message_contains="@type":"device"
    Should Have MQTT Messages    te/factory1/shop1/plc1/sensor1    message_contains="@id":"${DEVICE_SN}:factory1:shop1:plc1:sensor1"    message_contains="@type":"child-device"
    Should Have MQTT Messages    te/factory1/shop1/plc1/sensor2    message_contains="@id":"${DEVICE_SN}:factory1:shop1:plc1:sensor2"    message_contains="@type":"child-device"
    Should Have MQTT Messages    te/factory1/shop1/plc1/metrics    message_contains="@id":"${DEVICE_SN}:factory1:shop1:plc1:metrics"    message_contains="@type":"service"
    Should Have MQTT Messages    te/factory1/shop1/apps/sensor1    message_contains="@id":"${DEVICE_SN}:factory1:shop1:apps:sensor1"    message_contains="@type":"service"
    Should Have MQTT Messages    te/factory1/shop1/apps/sensor2    message_contains="@id":"${DEVICE_SN}:factory1:shop1:apps:sensor2"    message_contains="@type":"service"

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


Register tedge-agent when tedge-mapper-c8y is not running #2389
    [Teardown]    Start Service    tedge-mapper-c8y
    Device Should Exist    ${DEVICE_SN}

    Stop Service    tedge-mapper-c8y
    Execute Command    cmd=timeout 5 env TEDGE_RUN_LOCK_FILES=false tedge-agent --mqtt-device-topic-id device/offlinechild1//    ignore_exit_code=${True}
    Start Service    tedge-mapper-c8y

    Should Be A Child Device Of Device    ${DEVICE_SN}:device:offlinechild1
    Should Have MQTT Messages    te/device/offlinechild1//    minimum=1

    Device Should Exist    ${DEVICE_SN}:device:offlinechild1
    Cumulocity.Restart Device
    Should Have MQTT Messages    te/device/offlinechild1///cmd/restart/+

Early data messages cached and processed
    [Teardown]    Re-enable auto-registration and collect logs
    ${timestamp}=        Get Unix Timestamp
    ${prefix}=    Get Random Name
    Execute Command    sudo tedge config set c8y.entity_store.auto_register false
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

    ${children}=    Create List    child0    child00    child01    child000    child0000    child00000
    FOR    ${child}    IN    @{children}
        Execute Command    sudo tedge mqtt pub 'te/device/${child}///m/environment' '{ "temp": 50 }'
        Execute Command    sudo tedge mqtt pub 'te/device/${child}///twin/maintenance_mode' 'true'
    END

    Execute Command    tedge mqtt pub --retain 'te/device/child000//' '{"@type":"child-device","@id":"${prefix}child000","@parent": "device/child00//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/child00000//' '{"@type":"child-device","@id":"${prefix}child00000","@parent": "device/child0000//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/child0000//' '{"@type":"child-device","@id":"${prefix}child0000","@parent": "device/child000//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/child01//' '{"@type":"child-device","@id":"${prefix}child01","@parent": "device/child0//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/child00//' '{"@type":"child-device","@id":"${prefix}child00","@parent": "device/child0//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/child0//' '{"@type":"child-device","@id":"${prefix}child0"}'

    Check Child Device    ${DEVICE_SN}    ${prefix}child0    ${prefix}child0    thin-edge.io-child
    Check Child Device    ${prefix}child0    ${prefix}child00    ${prefix}child00    thin-edge.io-child
    Check Child Device    ${prefix}child0    ${prefix}child01    ${prefix}child01    thin-edge.io-child
    Check Child Device    ${prefix}child00    ${prefix}child000    ${prefix}child000    thin-edge.io-child
    Check Child Device    ${prefix}child000    ${prefix}child0000    ${prefix}child0000    thin-edge.io-child
    Check Child Device    ${prefix}child0000    ${prefix}child00000    ${prefix}child00000    thin-edge.io-child

    FOR    ${child}    IN    @{children}
        Cumulocity.Set Device    ${prefix}${child}
        Device Should Have Measurements    type=environment    minimum=1    maximum=1
        Device Should Have Fragments    maintenance_mode
    END

    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

Early data messages cached and processed without @id in registration messages
    [Teardown]    Re-enable auto-registration and collect logs
    ${timestamp}=        Get Unix Timestamp
    ${prefix}=    Get Random Name
    Execute Command    sudo tedge config set c8y.entity_store.auto_register false
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

    ${children}=    Create List    child0    child00    child01    child000    child0000    child00000
    FOR    ${child}    IN    @{children}
        Execute Command    sudo tedge mqtt pub 'te/device/${prefix}${child}///m/environment' '{ "temp": 50 }'
        Execute Command    sudo tedge mqtt pub 'te/device/${prefix}${child}///twin/maintenance_mode' 'true'
    END

    Execute Command    tedge mqtt pub --retain 'te/device/${prefix}child000//' '{"@type":"child-device","@parent": "device/${prefix}child00//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${prefix}child00000//' '{"@type":"child-device","@parent": "device/${prefix}child0000//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${prefix}child0000//' '{"@type":"child-device","@parent": "device/${prefix}child000//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${prefix}child01//' '{"@type":"child-device","@parent": "device/${prefix}child0//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${prefix}child00//' '{"@type":"child-device","@parent": "device/${prefix}child0//"}'
    Execute Command    tedge mqtt pub --retain 'te/device/${prefix}child0//' '{"@type":"child-device"}'

    Check Child Device    ${DEVICE_SN}    ${DEVICE_SN}:device:${prefix}child0    ${DEVICE_SN}:device:${prefix}child0    thin-edge.io-child
    Check Child Device    ${DEVICE_SN}:device:${prefix}child0    ${DEVICE_SN}:device:${prefix}child00    ${DEVICE_SN}:device:${prefix}child00    thin-edge.io-child
    Check Child Device    ${DEVICE_SN}:device:${prefix}child0    ${DEVICE_SN}:device:${prefix}child01    ${DEVICE_SN}:device:${prefix}child01    thin-edge.io-child
    Check Child Device    ${DEVICE_SN}:device:${prefix}child00    ${DEVICE_SN}:device:${prefix}child000    ${DEVICE_SN}:device:${prefix}child000    thin-edge.io-child
    Check Child Device    ${DEVICE_SN}:device:${prefix}child000    ${DEVICE_SN}:device:${prefix}child0000    ${DEVICE_SN}:device:${prefix}child0000    thin-edge.io-child
    Check Child Device    ${DEVICE_SN}:device:${prefix}child0000    ${DEVICE_SN}:device:${prefix}child00000    ${DEVICE_SN}:device:${prefix}child00000    thin-edge.io-child

    FOR    ${child}    IN    @{children}
        Cumulocity.Set Device    ${DEVICE_SN}:device:${prefix}${child}
        Device Should Have Measurements    type=environment    minimum=1    maximum=1
        Device Should Have Fragments    maintenance_mode
        Should Have MQTT Messages    te/device/${prefix}${child}//    message_contains="@id":"${DEVICE_SN}:device:${prefix}${child}"    message_contains="@type":"child-device"
    END

    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y

Entities persisted and restored
    [Teardown]    Enable clean start and collect logs
    Execute Command    sudo tedge config set c8y.entity_store.clean_start false
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y
    
    ${prefix}=    Get Random Name
    
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/' '{"@type":"child-device","@id":"${prefix}plc1"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc2/' '{"@type":"child-device","@id":"${prefix}plc2"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/sensor1' '{"@type":"child-device","@id":"${prefix}plc1-sensor1","@parent":"factory/shop/plc1/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/sensor2' '{"@type":"child-device","@id":"${prefix}plc1-sensor2","@parent":"factory/shop/plc1/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc2/sensor1' '{"@type":"child-device","@id":"${prefix}plc2-sensor1","@parent":"factory/shop/plc2/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/metrics' '{"@type":"service","@id":"${prefix}plc1-metrics","@parent":"factory/shop/plc1/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc2/metrics' '{"@type":"service","@id":"${prefix}plc2-metrics","@parent":"factory/shop/plc2/"}'

    External Identity Should Exist    ${prefix}plc1
    External Identity Should Exist    ${prefix}plc2
    External Identity Should Exist    ${prefix}plc1-sensor1
    External Identity Should Exist    ${prefix}plc1-sensor2
    External Identity Should Exist    ${prefix}plc2-sensor1
    External Identity Should Exist    ${prefix}plc1-metrics
    External Identity Should Exist    ${prefix}plc2-metrics

    Execute Command    cat /etc/tedge/.tedge-mapper-c8y/entity_store.jsonl
    ${original_last_modified_time}=    Execute Command    date -r /etc/tedge/.tedge-mapper-c8y/entity_store.jsonl

    FOR    ${counter}    IN RANGE    0    5
        ${timestamp}=    Get Unix Timestamp
        Restart Service    tedge-mapper-c8y
        Service Health Status Should Be Up    tedge-mapper-c8y

        # Assert that the file contents did not change on restart
        ${last_modified_time}=    Execute Command    date -r /etc/tedge/.tedge-mapper-c8y/entity_store.jsonl
        Should Be Equal    ${last_modified_time}    ${original_last_modified_time}

        # Assert that the restored entities are not converted again
        Should Have MQTT Messages    c8y/s/us    message_contains=101    date_from=${timestamp}    minimum=0    maximum=0
        Should Have MQTT Messages    c8y/s/us/${prefix}plc1    message_contains=101    date_from=${timestamp}    minimum=0    maximum=0
        Should Have MQTT Messages    c8y/s/us/${prefix}plc2    message_contains=101    date_from=${timestamp}    minimum=0    maximum=0
        Should Have MQTT Messages    c8y/s/us/${prefix}plc1    message_contains=102    date_from=${timestamp}    minimum=0    maximum=0
        Should Have MQTT Messages    c8y/s/us/${prefix}plc2    message_contains=102    date_from=${timestamp}    minimum=0    maximum=0
    END

Entities send to cloud on restart
    ${prefix}=    Get Random Name

    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/' '{"@type":"child-device","@id":"${prefix}plc1"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc2/' '{"@type":"child-device","@id":"${prefix}plc2"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/sensor1' '{"@type":"child-device","@id":"${prefix}plc1-sensor1","@parent":"factory/shop/plc1/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/sensor2' '{"@type":"child-device","@id":"${prefix}plc1-sensor2","@parent":"factory/shop/plc1/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc2/sensor1' '{"@type":"child-device","@id":"${prefix}plc2-sensor1","@parent":"factory/shop/plc2/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc1/metrics' '{"@type":"service","@id":"${prefix}plc1-metrics","@parent":"factory/shop/plc1/"}'
    Execute Command    tedge mqtt pub --retain 'te/factory/shop/plc2/metrics' '{"@type":"service","@id":"${prefix}plc2-metrics","@parent":"factory/shop/plc2/"}'

    External Identity Should Exist    ${prefix}plc1
    External Identity Should Exist    ${prefix}plc2
    External Identity Should Exist    ${prefix}plc1-sensor1
    External Identity Should Exist    ${prefix}plc1-sensor2
    External Identity Should Exist    ${prefix}plc2-sensor1
    External Identity Should Exist    ${prefix}plc1-metrics
    External Identity Should Exist    ${prefix}plc2-metrics

    ${timestamp}=    Get Unix Timestamp
    Restart Service    tedge-mapper-c8y
    Service Health Status Should Be Up    tedge-mapper-c8y
        
    # Assert that entities are sent to cloud again
    Should Have MQTT Messages    c8y/s/us    message_contains=101,${prefix}plc1    date_from=${timestamp}    minimum=1    maximum=1
    Should Have MQTT Messages    c8y/s/us    message_contains=101,${prefix}plc2    date_from=${timestamp}    minimum=1    maximum=1
    Should Have MQTT Messages    c8y/s/us/${prefix}plc1    message_contains=101,${prefix}plc1-sensor1    date_from=${timestamp}    minimum=1    maximum=1
    Should Have MQTT Messages    c8y/s/us/${prefix}plc1    message_contains=101,${prefix}plc1-sensor2    date_from=${timestamp}    minimum=1    maximum=1
    Should Have MQTT Messages    c8y/s/us/${prefix}plc2    message_contains=101,${prefix}plc2-sensor1    date_from=${timestamp}    minimum=1    maximum=1
    Should Have MQTT Messages    c8y/s/us/${prefix}plc1    message_contains=102,${prefix}plc1-metrics    date_from=${timestamp}    minimum=1    maximum=1
    Should Have MQTT Messages    c8y/s/us/${prefix}plc2    message_contains=102,${prefix}plc2-metrics    date_from=${timestamp}    minimum=1    maximum=1


*** Keywords ***

Should Have Retained Message Count  
    [Arguments]    ${topic}    ${exp_count}  
    ${output}=    Execute Command    mosquitto_sub --retained-only -W 3 -t "${topic}" -v    exp_exit_code=27    return_stdout=True  
    Length Should Be    ${output.splitlines()}    ${exp_count}  

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
    Should Be Equal    ${child_mo["owner"]}    device_${DEVICE_SN} 
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
