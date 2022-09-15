#Command to execute:    robot -d \results --timestampoutputs --log health_tedge_mapper.html --report NONE health_tedge_mapper.robot

*** Settings ***
Library    Browser
Library    SSHLibrary
Library    DateTime
Library    CryptoLibrary    variable_decryption=True
Library    Dialogs
Library    String
Library    CSVLibrary
Library    OperatingSystem
Suite Setup            Open Connection And Log In
Suite Teardown         SSHLibrary.Close All Connections

*** Variables ***
${HOST}           
${USERNAME}       pi
${PASSWORD}       crypt:LO3wCxZPltyviM8gEyBkRylToqtWm+hvq9mMVEPxtn0BXB65v/5wxUu7EqicpOgGhgNZVgFjY0o=          
${Version}        0.*
${download_dir}    /home/pi/
${url_dow}    https://github.com/thin-edge/thin-edge.io/actions
# ${user_git}    crypt:3Uk76kNdyyYOXxus2GFoLf8eRlt/W77eEkcSiswwh04HNfwt0NlJwI7ATKPABmxKk8K1a8NsI5QH0w8EmT8GWeqrFwX2    
# ${pass_git}    crypt:IcTs6FyNl16ThjeG6lql0zNTsjCAwg5s6PhjRrcEwQ9DVHHRB4TjrGcpblR6R1v7j9oUlL3RzwxGpfBfsijVnQ==    
${url}    https://qaenvironment.eu-latest.cumulocity.com/
${url_tedge}    qaenvironment.eu-latest.cumulocity.com
${user}    qatests
${pass}    crypt:34mpoxueRYy/gDerrLeBThQ2wp9F+2cw50XaNyjiGUpK488+1fgEfE6drOEcR+qZQ6dcjIWETukbqLU=    


*** Tasks ***
Go to root
    Run    cd
Install thin-edge.io on your device
    Create Timestamp
    Define Device id
    Uninstall tedge with purge
    Clear previous downloaded files if any
    Install_thin-edge
Set the URL of your Cumulocity IoT tenant
    ${rc}=    Execute Command    sudo tedge config set c8y.url ${url_tedge}    return_stdout=False    return_rc=True    #Set the URL of your Cumulocity IoT tenant
    Should Be Equal    ${rc}    ${0}

Create the certificate
    ${rc}=    Execute Command    sudo tedge cert create --device-id ${DeviceID}    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
    #You can then check the content of that certificate.
    ${output}=    Execute Command    sudo tedge cert show    #You can then check the content of that certificate.
    Should Contain    ${output}    Device certificate: /etc/tedge/device-certs/tedge-certificate.pem
    Should Contain    ${output}    Subject: CN=${DeviceID}, O=Thin Edge, OU=Test Device
    Should Contain    ${output}    Issuer: CN=${DeviceID}, O=Thin Edge, OU=Test Device
    Should Contain    ${output}    Valid from:
    Should Contain    ${output}    Valid up to:
    Should Contain    ${output}    Thumbprint:

tedge cert upload c8y command
    Write   sudo tedge cert upload c8y --user ${user}
    Write    ${pass}
    Sleep    3s

Connect the device
    ${output}=    Execute Command    sudo tedge connect c8y    #You can then check the content of that certificate.
    Sleep    3s
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

Sending your first telemetry data
    ${rc}=    Execute Command    tedge mqtt pub c8y/s/us 211,20    return_stdout=False    return_rc=True    #Set the URL of your Cumulocity IoT tenant
    Should Be Equal    ${rc}    ${0}

Download the measurements report file
    New Page    ${url}
    Wait For Elements State    //button[normalize-space()='Log in']    visible
    Click    //button[normalize-space()='Agree and proceed']
    Type Text    id=user    ${user}
    Type Text    id=password    ${pass}
    Click    //button[normalize-space()='Log in']
    Wait For Elements State    //i[@class='icon-2x dlt-c8y-icon-th']    visible
    Click    //i[@class='icon-2x dlt-c8y-icon-th']
    Wait For Elements State    //span[normalize-space()='Device management']    visible
    Click    //span[normalize-space()='Device management']
    Wait For Elements State    //span[normalize-space()='Devices']    visible
    Click    //span[normalize-space()='Devices']
    Wait For Elements State    //span[normalize-space()='All devices']    visible
    Click    //span[normalize-space()='All devices']
    Sleep    2s
    Wait For Elements State    div[ng-class='truncated-cell-content']    visible

    Click    //a[@title='${DeviceID}']

    Wait For Elements State    //span[normalize-space()='Measurements']    visible
    Click    //span[normalize-space()='Measurements']
    Wait For Elements State    //body/c8y-ui-root[@id='app']/c8y-bootstrap/div/div/div/div[@id='c8y-legacy-view']/div[@ng-if='vm.widthSet && vm.authState.hasAuth']/div[@ng-controller='measurementsCtrl as ctrl']/c8y-list-pagination[@items='supportedMeasurements']/div/div/c8y-measurements-fragment-chart[@fragment='m']/div/div/c8y-chart[@datapoints='vm.dataPoints']/div[2]//*[name()='svg'][1]/*[name()='rect'][1]
    Click    //span[contains(text(),'More…')]
    Click    (//button[@title='Download as CSV'][normalize-space()='Download as CSV'])[2]
    Wait For Elements State    //a[normalize-space()='Download']    visible
    ${dl_promise}          Promise To Wait For Download    /home/pi/report.zip
    Click    //a[normalize-space()='Download']
    ${file_obj}=    Wait For  ${dl_promise}
    Sleep    5s

Copy the downloaded report
    Put File    ${download_dir}report.zip

Unzip the report
    Execute Command    unzip report.zip
    Execute Command    rm *.zip
Get the report file name
    ${report}=    Execute Command    ls
    Set Suite Variable    ${report}

Delete downloaded zip
    Remove File    /home/pi/report.zip

Get the report csv
    SSHLibrary.Get File    ${report}
    Execute Command    rm *.csv

Read csv file and validate
    @{list}=  Read Csv File To List    ${report}
    Log  ${list[0]}
    Log  ${list[1]}
    Should Contain    ${list[1]}    ${DeviceID}
    Should Contain    ${list[1]}    c8y_TemperatureMeasurement.T
    Should Contain    ${list[1]}    20

Remove csv file
    Remove File    ${report}





*** Keywords ***
Open Connection And Log In
   Open Connection     ${HOST}
   Login               ${USERNAME}        ${PASSWORD}

Create Timestamp    #Creating timestamp to be used for Device ID
        ${timestamp}=    get current date    result_format=%d%m%Y%H%M%S
        log    ${timestamp}
        Set Global Variable    ${timestamp}
Define Device id    #Defining the Device ID, structure is (ST'timestamp') (eg. ST01092022091654)
        ${DeviceID}   Set Variable    ST${timestamp}
        Set Suite Variable    ${DeviceID}
Uninstall tedge with purge
    Execute Command    wget https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh
    Execute Command    chmod a+x uninstall-thin-edge_io.sh
    Execute Command    ./uninstall-thin-edge_io.sh purge
Clear previous downloaded files if any
    Execute Command    rm *.deb | rm *.zip | rm *.sh*
Install_thin-edge
    ${rc}=    Execute Command    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s    return_stdout=False    return_rc=True
    Should Be Equal    ${rc}    ${0}
