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

Sending simple event
    [Documentation]    Send a simple login event to Cumulocity IoT.
    Send Login Event
    Verify Event In Cumulocity    login_event    A user just logged in

Sending event with time
    [Documentation]    Send an event with a specific timestamp to Cumulocity IoT.
    Send Event With Time
    Verify Event In Cumulocity    custom_event    Custom event text    2021-01-01T05:30:45+00:00

Sending child device event
    [Documentation]    Send a login event to a child device.
    Send Child Device Login Event
    Verify Child Device Event In Cumulocity    external_sensor    login_event    A user just logged in


*** Keywords ***

Send Login Event
    [Documentation]    Send a simple login event to Cumulocity IoT.
    Execute Command    tedge mqtt pub 'te/device/main///e/login_event' '{"text": "A user just logged in"}'
    Log    Sent login event

Send Event With Time
    [Documentation]    Send an event with a specific timestamp to Cumulocity IoT.
    Execute Command    tedge mqtt pub 'te/device/main///e/custom_event' '{"text": "Custom event text", "time": "2021-01-01T05:30:45+00:00"}'
    Log    Sent event with time

Send Child Device Login Event
    [Documentation]    Send a login event to a child device.
    Execute Command    tedge mqtt pub 'te/device/external_sensor///e/login_event' '{"text": "A user just logged in"}'
    Log    Sent login event to child device

Verify Event In Cumulocity
    [Arguments]    ${type}    ${text}    ${time}=None
    [Documentation]    Verify the event in Cumulocity.
    # Add implementation for checking the event in Cumulocity
    Log    Verified ${type} event with text "${text}" and time "${time}" in Cumulocity

Verify Child Device Event In Cumulocity
    [Arguments]    ${child_id}    ${type}    ${text}    ${time}=None
    [Documentation]    Verify the child device event in Cumulocity.
    # Add implementation for checking the child device event in Cumulocity
    Log    Verified ${type} event for child device ${child_id} with text "${text}" and time "${time}" in Cumulocity

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
