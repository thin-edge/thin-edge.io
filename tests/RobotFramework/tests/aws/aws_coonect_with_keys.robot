*** Settings ***
Documentation       Verify that thin-edge.io can successfully connect to AWS IoT Core
...                 Test assumes that AWS credentials are added to the .env file
...                 AWS_HOST=
...                 AWS_ACCESS_KEY=
...                 AWS_SECRET_KEY=
...                 AWS_REGION=
...                 AWS_ACCOUNT=
   
Resource            ../../resources/common.resource
Library             ThinEdgeIO    #adapter=ssh
Library             OperatingSystem
# Library             String

Suite Setup         Custom Setup
Test Teardown       Custom Teardown

Test Tags           theme:aws    test:on_demand

*** Variables ***
${POLICY_NAME}=       thinedge.io
${CERT_PATH}=         /etc/tedge/device-certs/tedge-certificate.pem
${ROOT_CA_PATH}=      /etc/tedge/device-certs/tedge-certificate.pem

*** Test Cases ***

Create AWS IoT Policy and Thing  
    # Check adapter and run only if adapter is ssh
    Run Command If SSH Adapter

    # Verify the certificate creation
    ${cert_details}=  Execute Command  sudo tedge cert show
    Log  ${cert_details}
    
    # Create the AWS IoT policy file in a temporary format
    ${POLICY_FILE}=  Execute Command  mktemp  --suffix=.json
    Log  ${POLICY_FILE}
    Set Suite Variable    ${POLICY_FILE}
    Create AWS IoT Policy File  ${POLICY_FILE}
   
    # Create an AWS session
    Create Session With Keys  ${AWS_CONFIG.access_key}  ${AWS_CONFIG.secret_key}  ${AWS_CONFIG.region}
    
    # Create a new IoT policy
    ${policy_arn}=  Create New Policy  ${POLICY_NAME}  ${POLICY_FILE}

    # Verify that the policy was created
    ${policy_exists}=  Check Policy Exists  ${POLICY_NAME}
    Should Be True  ${policy_exists}  Policy ${POLICY_NAME} should exist after creation.
    
    # Register the device (thing) in AWS IoT
    ${thing_arn}=  Register Device  ${DEVICE_SN}

    # Verify that the device was created
    ${device_exists}=  Check Device Exists  ${DEVICE_SN}
    Log    ${device_exists}
    Should Be True  ${device_exists}  Device ${DEVICE_SN} should exist after creation.
    
    # Configure the device by attaching the policy and the certificate
    ${cert_data}=  Configure Device  ${DEVICE_SN}  ${POLICY_NAME}

    # Connect the device to AWS IoT Core 
    ${log}    Execute Command    sudo tedge connect aws
    Should Contain    ${log}    tedge-mapper-aws service successfully started and enabled!



*** Keywords ***

Create AWS IoT Policy File
    [Arguments]  ${POLICY_FILE}
    ${policy_content}=    Catenate
    ...    {
    ...        "Version": "2012-10-17",
    ...        "Statement": [
    ...            {
    ...                "Effect": "Allow",
    ...                "Action": "iot:Connect",
    ...                "Resource": "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:client/\${iot:Connection.Thing.ThingName}"
    ...            },
    ...            {
    ...                "Effect": "Allow",
    ...                "Action": "iot:Subscribe",
    ...                "Resource": [
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topicfilter/thinedge/\${iot:Connection.Thing.ThingName}/cmd/#",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topicfilter/\$aws/things/\${iot:Connection.Thing.ThingName}/shadow/#",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topicfilter/thinedge/devices/\${iot:Connection.Thing.ThingName}/test-connection"
    ...                ]
    ...            },
    ...            {
    ...                "Effect": "Allow",
    ...                "Action": "iot:Receive",
    ...                "Resource": [
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/thinedge/\${iot:Connection.Thing.ThingName}/cmd",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/thinedge/\${iot:Connection.Thing.ThingName}/cmd/*",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/\$aws/things/\${iot:Connection.Thing.ThingName}/shadow",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/\$aws/things/\${iot:Connection.Thing.ThingName}/shadow/*",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/thinedge/devices/\${iot:Connection.Thing.ThingName}/test-connection"
    ...                ]
    ...            },
    ...            {
    ...                "Effect": "Allow",
    ...                "Action": "iot:Publish",
    ...                "Resource": [
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/thinedge/\${iot:Connection.Thing.ThingName}/td",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/thinedge/\${iot:Connection.Thing.ThingName}/td/*",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/\$aws/things/\${iot:Connection.Thing.ThingName}/shadow",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/\$aws/things/\${iot:Connection.Thing.ThingName}/shadow/*",
    ...                    "arn:aws:iot: ${AWS_CONFIG.region}: ${AWS_CONFIG.account}:topic/thinedge/devices/\${iot:Connection.Thing.ThingName}/test-connection"
    ...                ]
    ...            }
    ...        ]
    ...    }
    OperatingSystem.Create File    ${POLICY_FILE}    ${policy_content}

Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=False
    Set Suite Variable    ${DEVICE_SN}
    ${log}    Execute Command    sudo tedge config set aws.url ${AWS_CONFIG.host}

Custom Teardown
    Teardown AWS Resources  ${POLICY_NAME}  ${DEVICE_SN}
    Execute Command  sudo rm -f ${CERT_PATH} ${ROOT_CA_PATH}
    OperatingSystem.Remove File    ${POLICY_FILE}
    Get Logs

Run Command If SSH Adapter
    ${thin_edge_io}=    Get Library Instance    ThinEdgeIO
    ${adapter}=    Call Method    ${thin_edge_io}    get_adapter
    Run Keyword If    '${adapter}' == 'ssh'    Run keyword and ignore error    Execute Command    sudo tedge cert remove | sudo tedge disconnect aws
    Run Keyword If    '${adapter}' == 'ssh'    Execute Command    sudo tedge cert create --device-id ${DEVICE_SN}
