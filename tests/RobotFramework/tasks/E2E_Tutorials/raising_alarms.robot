*** Settings ***

Resource              ../../../../resources/common.resource
Library               ThinEdgeIO    adapter=${ADAPTER}
Library               Cumulocity
Library               String
Suite Setup           Custom Setup
Suite Teardown        Custom Teardown

*** Variables ***

${ADAPTER}            ssh
${C8Y_ROOT_CERT_PATH}    /etc/ssl/certs

*** Tasks ***

Raise Alarm
    [Documentation]    This test case raises an alarm using ThinEdgeIO. 
    ...                It publishes an alarm message with specified type, 
    ...                text, and severity to the device and verifies the alarm's presence.
    Raise Alarm Keyword    Current_high    Current is high    critical

Raise Alarm With Timestamp
    [Documentation]    This test case raises an alarm with a specific timestamp using ThinEdgeIO. 
    ...                It publishes an alarm message with specified type, text, severity, 
    ...                and timestamp to the device and verifies the alarm's presence.
    Raise Alarm With Timestamp Keyword    temperature_high    Temperature is very high    critical   2021-01-01T05:30:45+00:00 

Raise Custom Alarm
    [Documentation]    This test case raises a custom alarm using Cumulocity. 
    ...                It publishes an alarm message with specified type, text, 
    ...                severity, and a custom fragment to the device and verifies the alarm's presence.
    Raise Custom Alarm Keyword    PIR    Person detected    critical    someOtherCustomFragment
    Raise Custom Alarm Keyword    PIR    Person detected    critical    someOtherCustomFragment

*** Keywords ***

Raise Alarm Keyword
    [Documentation]    Publishes an alarm message with the specified type, text, 
    ...                and severity to the device using ThinEdgeIO MQTT. 
    ...                Verifies that the alarm is present on the device.
    [Arguments]    ${alarm_type}    ${text}    ${severity}
    ${payload}=    Set Variable    "text": "${text}", "severity": "${severity}"
    Execute Command    tedge mqtt pub te/device/main///a/${alarm_type} '{${payload}}' -r -q 2
    Device Should Have Alarm/s    minimum=1    maximum=1    expected_text=${text}    type=${alarm_type}    severity=${severity}

Raise Alarm With Timestamp Keyword
    [Documentation]    Publishes an alarm message with the specified type, text, severity, 
    ...                and timestamp to the device using ThinEdgeIO MQTT. 
    ...                Verifies that the alarm is present on the device with the correct timestamp.
    [Arguments]    ${alarm_type1}    ${text1}    ${severity1}    ${timestamp1}
    ${payload}=    Set Variable    "text": "${text1}", "severity": "${severity1}", "time": "${timestamp1}"
    Execute Command    tedge mqtt pub te/device/main///a/${alarm_type1} '{${payload}}' -r -q 2
    Device Should Have Alarm/s    minimum=1    maximum=1    expected_text=${text1}    type=${alarm_type1}    severity=${severity1}

Raise Custom Alarm Keyword
    [Documentation]    Publishes a custom alarm message with the specified type, text, 
    ...                severity, and a custom fragment to the device using ThinEdgeIO MQTT. 
    ...                Verifies that the alarm is present on the device.
    [Arguments]    ${alarm_type}    ${text}    ${severity}    ${custom_fragment}
    ${payload}=    Set Variable    "text": "${text}", "severity": "${severity}", "${custom_fragment}": "custom_value"
    Execute Command    tedge mqtt pub te/device/main///a/${alarm_type} '{${payload}}' -r -q 2

Custom Setup
    [Documentation]    Initializes the device environment. 
    ...                Sets up the device, transfers necessary packages, 
    ...                installs them, and configures Cumulocity for connectivity.
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    ${DEVICE_SN}
    ${log}    Transfer To Device    target/aarch64-unknown-linux-musl/packages/*.deb    /home/pi/
    Execute Command    sudo dpkg -i *.deb
    Log    Installed new packages on device
    Configure Cumulocity

Custom Teardown
    [Documentation]    Cleans up the device environment. 
    ...                Uninstalls ThinEdgeIO, removes packages and scripts, and retrieves logs.
    Transfer To Device    ${CURDIR}/uninstall-thin-edge_io.sh    /home/pi/uninstall-thin-edge_io.sh
    Execute Command    sudo chmod a+x uninstall-thin-edge_io.sh
    Execute Command    ./uninstall-thin-edge_io.sh purge
    Log    Successfully uninstalled with purge
    Execute Command    sudo rm -rf /home/pi/*.deb
    Execute Command    sudo rm -rf /home/pi/*.sh
    Get Logs

Configure Cumulocity
    [Documentation]    Configures the Cumulocity IoT connection settings on the device. 
    ...                Sets the Cumulocity URL, uploads the certificate, and connects the device to Cumulocity.
    ${HOSTNAME}=      Replace String Using Regexp    ${C8Y_CONFIG.host}    ^.*://    ${EMPTY}
    ${HOSTNAME}=      Strip String    ${HOSTNAME}    characters=/
    Execute Command    sudo tedge config set c8y.url ${HOSTNAME}
    Execute Command    sudo tedge config set c8y.root.cert.path ${C8Y_ROOT_CERT_PATH}
    Execute Command    sudo tedge cert create --device-id ${DEVICE_SN}
    Execute Command    sudo env C8YPASS\='${C8Y_CONFIG.password}' tedge cert upload c8y --user ${C8Y_CONFIG.username}
    Execute Command    sudo tedge connect c8y
    Sleep    3s    reason=Wait for cert to be processed/distributed to all cores (in Cumulocity IoT)
