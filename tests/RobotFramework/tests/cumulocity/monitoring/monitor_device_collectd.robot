*** Settings ***
Resource        ../../../resources/common.resource
Library         ThinEdgeIO
Library         Cumulocity

Suite Setup     Custom Setup
Test Teardown    Get Logs

Force Tags      theme:monitoring    theme:c8y    theme:collectd

*** Test Cases ***
Check running collectd
    Service Should Be Running    collectd
    
Is collectd publishing MQTT messages?
    ${messages}=    Should Have MQTT Messages    topic=collectd/#    minimum=1    maximum=None
    # Should Contain    ${messages[0]}    "pid":${pid}
    # Should Contain    ${messages[0]}    "status":"up"

Check thin-edge monitoring
    Execute Command    sudo systemctl enable tedge-mapper-collectd
    Execute Command    sudo systemctl start tedge-mapper-collectd
    ${tedge_messages}=    Should Have MQTT Messages    topic=tedge/measurements    minimum=1    maximum=None
    ${c8y_messages}=    Should Have MQTT Messages    topic=c8y/#    minimum=1    maximum=None
    

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}
    Execute Command    sudo apt-get --assume-yes install collectd-core && sudo cp /etc/tedge/contrib/collectd/collectd.conf /etc/collectd/collectd.conf
    Execute Command    sudo systemctl restart collectd
