*** Settings ***
Resource            ../../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:operation    theme:custom    theme:smartrest    theme:template


*** Test Cases ***
smartrest template custom operation successful
    Create SmartREST2 Template    ${CURDIR}/set_wifi.json    ${DEVICE_SN}
    Should Have SmartREST2 Template    ${DEVICE_SN}

    ${operation}=    Cumulocity.Create Operation
    ...    description=do something
    ...    fragments={"set_wifi":{"name":"Factory Wifi","ssid":"factory-onboarding-wifi","type":"WPA3-Personal"}}
    Operation Should Be SUCCESSFUL    ${operation}

    ${c8y_messages}=    Should Have MQTT Messages
    ...    c8y/s/dc/${DEVICE_SN}
    ...    minimum=1
    ...    maximum=1
    ...    message_contains=dm101,${DEVICE_SN},Factory Wifi,factory-onboarding-wifi,WPA3-Personal


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    ThinEdgeIO.Transfer To Device    ${CURDIR}/set_wifi    /etc/tedge/operations/c8y/set_wifi
    Execute Command    sed -i -e 's/custom_devmgmt/${DEVICE_SN}/g' /etc/tedge/operations/c8y/set_wifi

    ThinEdgeIO.Transfer To Device    ${CURDIR}/set_wifi.sh    /etc/tedge/operations/set_wifi.sh
    Execute Command    chmod a+x /etc/tedge/operations/set_wifi.sh

    Execute Command    tedge config set c8y.smartrest.templates ${DEVICE_SN}
    Execute Command    tedge reconnect c8y
