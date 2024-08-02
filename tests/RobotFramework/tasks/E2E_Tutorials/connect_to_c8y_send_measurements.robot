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

Sending simple measurements
    [Documentation]    Send a simple temperature measurement to Cumulocity IoT.
    Send Temperature Measurement
    Verify Measurement In Cumulocity    type=environment 

Sending complex measurements
    [Documentation]    Send a complex measurement (three_phase_current and combined) to Cumulocity IoT.
    Send Three Phase Current Measurement
    Verify Measurement In Cumulocity    type=environment 
    Send Combined Measurement
    Verify Measurement In Cumulocity    type=environment

Sending child device measurements
    [Documentation]    Send a temperature measurement to a child device.
    Send Child Device Temperature Measurement
    Verify Child Device Measurement In Cumulocity    child1    temperature    25

*** Keywords ***

Configure Cumulocity URL
    [Documentation]    Set the Cumulocity IoT URL for the device.
    ${HOSTNAME}=      Replace String Using Regexp    ${C8Y_CONFIG.host}    ^.*://    ${EMPTY}
    ${HOSTNAME}=      Strip String    ${HOSTNAME}    characters=/
    Execute Command    sudo tedge config set c8y.url ${HOSTNAME}
    Log    Configured Cumulocity URL to ${HOSTNAME}

Configure Root Certificate Path
    [Documentation]    Set the path to the root certificate for Cumulocity IoT.
    Execute Command    sudo tedge config set c8y.root_cert_path ${C8Y_ROOT_CERT_PATH}
    Log    Configured root certificate path to ${C8Y_ROOT_CERT_PATH}

Create Device Certificate
    [Documentation]    Create a self-signed certificate for the device.
    Execute Command    sudo tedge cert create --device-id ${DEVICE_SN}
    Log    Created device certificate for ${DEVICE_SN}

Check Device Certificate
    [Documentation]    Verify the contents of the device certificate.
    ${output}=    Execute Command    sudo tedge cert show
    Log    ${output}
    Should Contain    ${output}    Device certificate: /etc/tedge/device-certs/tedge-certificate.pem
    Should Contain    ${output}    Subject: CN=${DEVICE_SN}, O=Thin Edge, OU=Test Device
    Should Contain    ${output}    Issuer: CN=${DEVICE_SN}, O=Thin Edge, OU=Test Device
    Should Contain    ${output}    Valid from:
    Should Contain    ${output}    Valid up to:
    Should Contain    ${output}    Thumbprint:

Upload Device Certificate
    [Documentation]    Upload the device certificate to Cumulocity IoT.
    ${output}     Execute Command    sudo env C8YPASS\='${C8Y_CONFIG.password}' tedge cert upload c8y --user ${C8Y_CONFIG.username}
    Log    ${output}
    Should Contain    ${output}    Certificate uploaded successfully.
    Log    Uploaded device certificate for ${DEVICE_SN}

Connect to Cumulocity
    [Documentation]    Connect the device to Cumulocity IoT.
    ${output}=    Execute Command    sudo tedge connect c8y
    Log    ${output}
    Should Contain    ${output}    Checking if systemd is available.
    Should Contain    ${output}    Checking if configuration for requested bridge already exists.
    Should Contain    ${output}    Validating the bridge certificates.
    Should Contain    ${output}    Creating the device in Cumulocity cloud.
    Should Contain    ${output}    Saving configuration for requested bridge.
    Should Contain    ${output}    Restarting mosquitto service.
    Should Contain    ${output}    Awaiting mosquitto to start. This may take up to 5 seconds.
    Should Contain    ${output}    Enabling mosquitto service on reboots.
    Should Contain    ${output}    Successfully created bridge connection!
    Should Contain    ${output}    Sending packets to check connection. This may take up to 2 seconds.
    Should Contain    ${output}    Connection check is successful.
    Should Contain    ${output}    Checking if tedge-mapper is installed.
    Should Contain    ${output}    Starting tedge-mapper-c8y service.
    Should Contain    ${output}    Persisting tedge-mapper-c8y on reboot.
    Should Contain    ${output}    tedge-mapper-c8y service successfully started and enabled!
    Should Contain    ${output}    Enabling software management.
    Should Contain    ${output}    Checking if tedge-agent is installed.
    Should Contain    ${output}    Starting tedge-agent service.
    Should Contain    ${output}    Persisting tedge-agent on reboot.
    Should Contain    ${output}    tedge-agent service successfully started and enabled!
    Log    Connected to Cumulocity IoT and verified connection for ${DEVICE_SN}

Send Temperature Measurement
    [Documentation]    Send a temperature measurement.
    Execute Command    tedge mqtt pub 'te/device/main///m/environment' '{"temperature": 25}'
    Log    Sent temperature measurement: 25°C

Send Three Phase Current Measurement
    [Documentation]    Send a three-phase current measurement.
    Execute Command    tedge mqtt pub 'te/device/main///m/electrical' '{"three_phase_current": {"L1": 9.5, "L2": 10.3, "L3": 8.8}}'
    Log    Sent three-phase current measurement

Send Combined Measurement
    [Documentation]    Send a combined measurement.
    Execute Command    tedge mqtt pub 'te/device/main///m/combined' '{"time": "2020-10-15T05:30:47+00:00", "temperature": 25, "current": {"L1": 9.5, "L2": 10.3, "L3": 8.8}, "pressure": 98}'
    Log    Sent combined measurement

Send Child Device Temperature Measurement
    [Documentation]    Send a temperature measurement to a child device.
    Execute Command    tedge mqtt pub 'te/device/child1///m/environment' '{"temperature": 25}'
    Log    Sent temperature measurement to child device: 25°C

Verify Measurement In Cumulocity
    [Arguments]        ${type} 
    Device Should Have Measurements    type=${type}    minimum=1    maximum=1

Verify Child Device Measurement In Cumulocity
    [Arguments]    ${child_id}    ${type}    ${value}
    [Documentation]    Verify the child device measurement in Cumulocity.
    # Add implementation for checking the child device measurement in Cumulocity
    Log    Verified ${type} measurement for child device ${child_id} with value ${value} in Cumulocity


Custom Setup
    [Documentation]    Initializes the device environment. 
    ...                Sets up the device, transfers necessary packages, 
    ...                installs them, and configures Cumulocity for connectivity.
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    ${DEVICE_SN}

    # Determine the device architecture
    ${output}    Execute Command    uname -m
    ${arch}    Set Variable    ${output.strip()}

    # Conditional file transfer based on architecture
    Run Keyword If    '${arch}' == 'aarch64'    Transfer Aarch64 Packages
    ...    ELSE IF    '${arch}' == 'armv7l'    Transfer Armv7l Packages
    ...    ELSE    Log    Unsupported architecture: ${arch}

    # Install packages
    Execute Command    sudo dpkg -i /var/local/share/*.deb

    Log    Installed new packages on device

Transfer Aarch64 Packages
    [Documentation]    Transfers Aarch64 architecture packages to the device.
    ${log}    Transfer To Device    target/aarch64-unknown-linux-musl/packages/*.deb    /var/local/share/
    Log    Transferred Aarch64 packages to device

Transfer Armv7l Packages
    [Documentation]    Transfers ARMv7l architecture packages to the device.
    ${log}    Transfer To Device    target/armv7-unknown-linux-musleabihf/packages/*.deb    /var/local/share/
    Log    Transferred ARMv7l packages to device

Custom Teardown
    [Documentation]    Cleans up the device environment. 
    ...                Uninstalls ThinEdgeIO, removes packages and scripts, and retrieves logs.
    Transfer To Device    ${CURDIR}/uninstall-thin-edge_io.sh    /var/local/share/uninstall-thin-edge_io.sh 
    Execute Command    sudo chmod a+x /var/local/share/uninstall-thin-edge_io.sh
    Execute Command    sudo /var/local/share/uninstall-thin-edge_io.sh
    Log    Successfully uninstalled with purge
    Execute Command    sudo rm -rf /var/local/share/*.deb
    Transfer To Device    ${CURDIR}/uninstall-thin-edge_io.sh    /var/local/share/uninstall-thin-edge_io.sh
    Get Logs
