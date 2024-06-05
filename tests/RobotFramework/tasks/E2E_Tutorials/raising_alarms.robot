*** Settings ***

Resource              ../../../../resources/common.resource
Library               ThinEdgeIO    adapter=${ADAPTER}
Library               Cumulocity
Library               String
Suite Setup           Custom Setup
Suite Teardown        Get Logs

*** Variables ***

${ADAPTER}            ssh
${C8Y_ROOT_CERT_PATH}    /etc/ssl/certs

*** Tasks ***

Configure the device
    [Documentation]    Configure the device with the Cumulocity IoT URL and root certificate path.
    Configure Cumulocity URL
    Configure Root Certificate Path

Create the certificate
    [Documentation]    Create a self-signed certificate for the device and verify its contents.
    Create Device Certificate
    Check Device Certificate

Make the device trusted by Cumulocity
    [Documentation]    Upload the device certificate to Cumulocity and ensure it's trusted.
    Upload Device Certificate
    Sleep    3s    reason=Wait for cert to be processed/distributed to all cores (in Cumulocity IoT)

Connect the device
    [Documentation]    Connect the device to Cumulocity IoT and verify the connection.
    Connect to Cumulocity
    Device Should Exist    ${DEVICE_SN}

Raise Alarm
    [Documentation]    Raise an alarm using ThinEdgeIO
    Raise Alarm Keyword    temperature_high    Temperature is very high    critical

Raise Alarm With Timestamp
    [Documentation]    Raise an alarm with a specific timestamp
    Raise Alarm With Timestamp Keyword    temperature_high    Temperature is very high    critical    2024-01-01T12:00:00Z

Raise Custom Alarm
    [Documentation]    Raise a custom alarm using Cumulocity
    Raise Custom Alarm Keyword    temperature_high    Temperature is very high    critical    2024-01-01T12:00:00Z    someOtherCustomFragment

*** Keywords ***

Raise Alarm Keyword
    [Arguments]    ${alarm_type}    ${text}    ${severity}
    ${payload}=    Set Variable    {"text": "${text}", "severity": "${severity}"}
    Execute Command    tedge mqtt pub te/device/main///a/${alarm_type} '{${payload}}' -r -q 2

Raise Alarm With Timestamp Keyword
    [Arguments]    ${alarm_type}    ${text}    ${severity}    ${timestamp}
    ${payload}=    Set Variable    {"text": "${text}", "severity": "${severity}", "time": "${timestamp}"}
    Execute Command    tedge mqtt pub te/device/main///a/${alarm_type} '{${payload}}' -r -q 2

Raise Custom Alarm Keyword
    [Arguments]    ${alarm_type}    ${text}    ${severity}    ${timestamp}    ${custom_fragment}
    ${payload}=    Set Variable    {"text": "${text}", "severity": "${severity}", "time": "${timestamp}", "${custom_fragment}": "custom_value"}
    Execute Command    tedge mqtt pub te/device/main///a/${alarm_type} '{${payload}}' -r -q 2

Configure Cumulocity URL
    [Documentation]    Set the Cumulocity IoT URL for the device.
    ${HOSTNAME}=      Replace String Using Regexp    ${C8Y_CONFIG.host}    ^.*://    ${EMPTY}
    ${HOSTNAME}=      Strip String    ${HOSTNAME}    characters=/
    Execute Command    sudo tedge config set c8y.url ${HOSTNAME}
    Log    Configured Cumulocity URL to ${HOSTNAME}

Configure Root Certificate Path
    [Documentation]    Configure the root certificate path on the device.
    Execute Command    tedge config set c8y.root.cert.path ${C8Y_ROOT_CERT_PATH}
    Log    Configured root certificate path: ${C8Y_ROOT_CERT_PATH}

Create Device Certificate
    [Documentation]    Create a self-signed certificate for the device.
    Execute Command    tedge cert create --device-id ${DEVICE_SN}
    Log    Created device certificate for: ${DEVICE_SN}

Check Device Certificate
    [Documentation]    Check the contents of the device certificate.
    Execute Command    tedge cert show
    Log    Verified device certificate

Upload Device Certificate
    [Documentation]    Upload the device certificate to Cumulocity IoT.
    ${output}     Execute Command    sudo env C8YPASS\='${C8Y_CONFIG.password}' tedge cert upload c8y --user ${C8Y_CONFIG.username}
    Log    ${output}
    Should Contain    ${output}    Certificate uploaded successfully.
    Log    Uploaded device certificate for ${DEVICE_SN}

Connect to Cumulocity
    [Documentation]    Connect the device to Cumulocity IoT.
    Execute Command    tedge connect c8y
    Log    Connected to Cumulocity IoT

Custom Setup
    [Documentation]    Custom setup for initializing the device environment.
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    ${DEVICE_SN}
    Execute Command    sudo rm -rf /home/pi/*.deb
    Transfer To Device    ${CURDIR}/uninstall-thin-edge_io.sh    /home/pi/uninstall-thin-edge_io.sh
    Execute Command    chmod a+x uninstall-thin-edge_io.sh
    Execute Command    ./uninstall-thin-edge_io.sh purge
    Log    Successfully uninstalled with purge
    ${log}    Transfer To Device    target/aarch64-unknown-linux-musl/packages/*.deb    /home/pi/
    Execute Command    sudo dpkg -i *.deb
    Log    Installed new packages on device
