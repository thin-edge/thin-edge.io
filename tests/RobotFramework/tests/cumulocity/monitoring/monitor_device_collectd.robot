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
...                Checking the grouping of measurements:
...                Sending fake collectd measurements with stopped collectd and check that messages are
...                published on tedge/measurements for that timestamp and grouping the two fake collectd measurements 
...                (i.e. the temperature and the pressure sent by the two fake measurements).


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

Check thin-edge monitoring
    # 3. Enable thin-edge.io monitoring.
    Execute Command    sudo systemctl enable tedge-mapper-collectd
    Execute Command    sudo systemctl start tedge-mapper-collectd
    # Check thin-edge monitoring
    ${tedge_messages}=    Should Have MQTT Messages    topic=tedge/measurements    minimum=1    maximum=None
    Should Contain    ${tedge_messages[0]}   "time"
    Should Contain Any    ${tedge_messages[0]}    "memory"    "cpu"    "df-root"
    ${c8y_messages}=    Should Have MQTT Messages    topic=c8y/measurement/measurements/create    minimum=1    maximum=None
    Should Contain    ${c8y_messages[0]}    "type":"ThinEdgeMeasurement"

Check grouping of measurements 
    # This test step is only partially checking the grouping of the messages, because of the timeouts and the current design
    # if proper checks will be implemented this test would be failing from time to time
    
    Execute Command    sudo systemctl stop collectd
    Sleep    5s    reason=Needed because of the batching
    ${start_time}=    Get Unix Timestamp
    Execute Command    tedge mqtt pub collectd/localhost/temperature/temp1 "`date +%s.%N`:50" && tedge mqtt pub collectd/localhost/temperature/temp2 "`date +%s.%N`:40" && tedge mqtt pub collectd/localhost/pressure/pres1 "`date +%s.%N`:10" && tedge mqtt pub collectd/localhost/pressure/pres2 "`date +%s.%N`:20"
    ${c8y_messages}    Should Have MQTT Messages    c8y/measurement/measurements/create    maximum=4    date_from=${start_time}
    Should Contain Any   ${c8y_messages[0]}    "temp1":{"value":50.0}    "temp2":{"value":40.0}    "pres1":{"value":10.0}    "pres2":{"value":20.0}


*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}
    # 1. Install collectd
    # 2. Configure collectd
    Execute Command    sudo apt-get install libmosquitto1
    Execute Command    sudo apt-get --assume-yes install collectd-core && sudo cp /etc/tedge/contrib/collectd/collectd.conf /etc/collectd/collectd.conf
    Execute Command    sudo systemctl restart collectd
