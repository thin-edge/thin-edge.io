*** Settings ***
Resource    ../../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:c8y    theme:operation
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Register SmartREST 1 templates
    ${TEMPLATE_XID}=    Set Variable    templateXIDexample01
    # Execute Command    cmd=curl -XPOST -H "X-Id: templateXIDexample01" -d "10,107,GET,/inventory/managedObjects/%%/childDevices?pageSize=100,,,%%,," http://127.0.0.1:8001/c8y/s
    Execute Command    cmd=tedge mqtt pub c8y/s/ul "15,${TEMPLATE_XID}\n10,107,GET,/inventory/managedObjects/%%/childDevices?pageSize=100,,,%%,,\n"
    Log    debug
    # 10,107,GET,/inventory/managedObjects/%%/childDevices?pageSize=100,,,%%,,\n
    # Should Have MQTT Messages    c8y/s/us    message_pattern=114,c8y_DownloadConfigFile,c8y_LogfileRequest,c8y_RemoteAccessConnect,c8y_Restart,c8y_SoftwareUpdate,c8y_UploadConfigFile    minimum=1    maximum=1
   

*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
