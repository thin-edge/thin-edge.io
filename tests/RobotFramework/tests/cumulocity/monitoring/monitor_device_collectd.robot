*** Settings ***
Documentation      With thin-edge.io device monitoring, you can collect metrics from your device 
...                and forward these device metrics to IoT platforms in the cloud. Using these metrics, 
...                you can monitor the health of devices and can proactively initiate actions in case the 
...                device seems to malfunction. Additionally, the metrics can be used to help the customer 
...                troubleshoot when problems with the device are reported. Thin-edge.io uses the open source 
...                component collectd to collect the metrics from the device. Thin-edge.io translates the 
...                collected metrics from their native format to the thin-edge.io JSON format and then into 
...                the cloud-vendor specific format.
...                Enabling monitoring on your device is a 3-steps process:
...                1. Install collectd,
...                2. Configure collectd,
...                3. Enable thin-edge.io monitoring.


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
    # 3. Enable thin-edge.io monitoring.
    Execute Command    sudo systemctl enable tedge-mapper-collectd
    Execute Command    sudo systemctl start tedge-mapper-collectd
    # Check thin-edge monitoring
    ${tedge_messages}=    Should Have MQTT Messages    topic=tedge/measurements    minimum=1    maximum=None
    ${c8y_messages}=    Should Have MQTT Messages    topic=c8y/measurement/measurements/create    minimum=1    maximum=None
    
*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}
    # 1. Install collectd
    # 2. Configure collectd
    Execute Command    sudo apt-get --assume-yes install collectd-core && sudo cp /etc/tedge/contrib/collectd/collectd.conf /etc/collectd/collectd.conf
    Execute Command    sudo systemctl restart collectd
