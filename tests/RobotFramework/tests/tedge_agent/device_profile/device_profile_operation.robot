*** Comments ***
# robocop: off = misaligned-continuation-row


*** Settings ***
Resource            ../../../resources/common.resource
Library             Collections
Library             OperatingSystem
Library             ThinEdgeIO
Library             Cumulocity

Suite Setup         Custom Setup
Test Setup          Custom Test Setup
Test Teardown       Get Logs

Test Tags           theme:tedge_agent


*** Test Cases ***
Send device profile operation from Cumulocity
    ${config_url}=    Create Inventory Binary
    ...    tedge-configuration-plugin
    ...    tedge-configuration-plugin
    ...    file=${CURDIR}/tedge-configuration-plugin.toml

    ${PROFILE_NAME}=    Set Variable    Test Profile
    # robocop: off=misaligned-continuation-row
    # robotidy: off
    ${PROFILE_PAYLOAD}=    Catenate    SEPARATOR=\n
    ...    {
    ...      "firmware": {
    ...        "name": "tedge-core",
    ...        "version": "1.0.0",
    ...        "url": "https://abc.com/some/firmware/url"
    ...      },
    ...      "software": [
    ...        {
    ...          "name": "jq",
    ...          "action": "install",
    ...          "version": "latest",
    ...          "url": ""
    ...        },
    ...        {
    ...          "name": "tree",
    ...          "action": "install",
    ...          "version": "latest",
    ...          "url": ""
    ...        }
    ...      ],
    ...      "configuration": [
    ...        {
    ...          "name": "tedge-configuration-plugin",
    ...          "type": "tedge-configuration-plugin",
    ...          "url": "${config_url}"
    ...        }
    ...      ]
    ...    }

    ${profile}=    Cumulocity.Create Device Profile    ${PROFILE_NAME}    ${PROFILE_PAYLOAD}
    ${operation}=    Cumulocity.Install Device Profile    ${profile["id"]}
    ${operation}=    Cumulocity.Operation Should Be SUCCESSFUL    ${operation}
    Cumulocity.Should Have Device Profile Installed    ${profile["id"]}

    Cumulocity.Device Should Have Firmware    tedge-core    1.0.0

    Device Should Have Installed Software    {"name": "jq"}    {"name": "tree"}

    ${mo}=    Managed Object Should Have Fragments   c8y_Configuration_tedge-configuration-plugin
    Should Be Equal    ${mo["c8y_Configuration_tedge-configuration-plugin"]["name"]}    tedge-configuration-plugin
    Should Be Equal    ${mo["c8y_Configuration_tedge-configuration-plugin"]["type"]}    tedge-configuration-plugin
    Should Be Equal    ${mo["c8y_Configuration_tedge-configuration-plugin"]["url"]}    ${config_url}

Send device profile operation locally
    ${config_url}=    Set Variable    http://localhost:8000/tedge/v1/files/main/config_update/robot-123

    Execute Command    curl -X PUT --data-binary "bad toml" "${config_url}"

    # robocop: off=misaligned-continuation-row
    # robotidy: off
    ${payload}=    Catenate    SEPARATOR=\n
    ...    {
    ...      "status": "init",
    ...      "name": "dev-profile",
    ...      "version": "v2",
    ...      "operations": [
    ...        {
    ...          "operation": "firmware_update",
    ...          "@skip": false,
    ...          "payload": {
    ...            "name": "tedge-core",
    ...            "remoteUrl": "https://abc.com/some/firmware/url",
    ...            "version": "1.0.0"
    ...          }
    ...        },
    ...        {
    ...          "operation": "software_update",
    ...          "@skip": false,
    ...          "payload": {
    ...            "updateList": [
    ...              {
    ...                "type": "apt",
    ...                "modules": [
    ...                  {
    ...                    "name": "yq",
    ...                    "version": "latest",
    ...                    "action": "install"
    ...                  },
    ...                  {
    ...                    "name": "jo",
    ...                    "version": "latest",
    ...                    "action": "install"
    ...                  }
    ...                ]
    ...              }
    ...            ]
    ...          }
    ...        },
    ...        {
    ...          "operation": "config_update",
    ...          "@skip": false,
    ...          "payload": {
    ...            "type": "tedge-configuration-plugin",
    ...            "tedgeUrl": "${config_url}",
    ...            "remoteUrl": "",
    ...            "serverUrl": ""
    ...          }
    ...        },
    ...        {
    ...          "operation": "software_update",
    ...          "@skip": true,
    ...          "payload": {
    ...            "updateList": [
    ...              {
    ...                "type": "apt",
    ...                "modules": [
    ...                  {
    ...                    "name": "htop",
    ...                    "version": "latest",
    ...                    "action": "install"
    ...                  }
    ...                ]
    ...              }
    ...            ]
    ...          }
    ...        },
    ...        {
    ...          "operation": "restart",
    ...          "skip": false,
    ...          "payload": {}
    ...        },
    ...        {
    ...          "operation": "software_update",
    ...          "@skip": false,
    ...          "payload": {
    ...            "updateList": [
    ...              {
    ...                "type": "apt",
    ...                "modules": [
    ...                  {
    ...                    "name": "rolldice",
    ...                    "version": "latest",
    ...                    "action": "install"
    ...                  }
    ...                ]
    ...              }
    ...            ]
    ...          }
    ...        }
    ...      ]
    ...    }

    Execute Command    tedge mqtt pub --retain 'te/device/main///cmd/device_profile/robot-123' '${payload}'
    ${cmd_messages}=    Should Have MQTT Messages
    ...    te/device/main///cmd/device_profile/robot-123
    ...    message_pattern=.*successful.*
    ...    maximum=1
    ...    timeout=60

    # Validate installed packages
    Execute Command    dpkg -l | grep rolldice
    Execute Command    dpkg -l | grep yq
    Execute Command    dpkg -l | grep jo

    # Validate tree package is not installed
    Execute Command    dpkg -l | grep htop    exp_exit_code=1

    # Validate updated config file
    Execute Command    grep "bad toml" /etc/tedge/plugins/tedge-configuration-plugin.toml

    ${twin_messages}=    Should Have MQTT Messages
    ...    te/device/main///twin/device_profile
    ...    message_pattern=.*"name": "dev-profile",.*"version": "v2"
    ...    maximum=1
    ...    timeout=60

    [Teardown]    Execute Command    tedge mqtt pub --retain te/device/main///cmd/device_profile/robot-123 ''


*** Keywords ***
Custom Test Setup
    Execute Command
    ...    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /usr/bin/systemctl, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh, /usr/bin/tedge-write /etc/*' > /etc/sudoers.d/tedge

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    Copy Configuration Files
    Restart Service    tedge-agent

    # setup repos so that packages can be installed from them
    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.deb.sh' | sudo -E bash
    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.deb.sh' | sudo -E bash

Copy Configuration Files
    ThinEdgeIO.Transfer To Device    ${CURDIR}/firmware_update.toml    /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge_operator_helper.sh    /etc/tedge/operations/
