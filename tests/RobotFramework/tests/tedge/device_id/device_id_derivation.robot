*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Suite Setup
Test Setup          Custom Test Setup
Test Teardown       Custom Teardown

Test Tags           theme:cli


*** Test Cases ***
All use default cert/key paths
    Execute Command    tedge config unset c8y.device.cert_path
    Execute Command    tedge config unset c8y.device.key_path
    Execute Command    tedge config unset c8y.device.cert_path --profile foo
    Execute Command    tedge config unset c8y.device.key_path --profile foo

    Execute Command    tedge config get device.id    exp_exit_code=1
    ${output}=    Execute Command    tedge cert create --device-id input
    Should Contain    ${output}    CN=input
    Validate device IDs    input    input    input

Create a certificate without device.id in tedge config settings
    Execute Command    tedge config get device.id    exp_exit_code=1
    ${output}=    Execute Command    tedge cert create --device-id input
    Should Contain    ${output}    CN=input
    ${output}=    Execute Command    tedge config get device.id    strip=${True}
    Should Be Equal    ${output}    input
    Execute Command    tedge config get c8y.device.id    exp_exit_code=1
    Execute Command    tedge config get c8y.device.id --profile foo    exp_exit_code=1

Input from --device-id is used over the value from tedge config settings when device.id is set
    Execute Command    tedge config set device.id testid
    Validate device IDs    testid    testid    testid

    ${output}=    Execute Command    tedge cert create --device-id different
    Should Contain    ${output}    CN=different
    Validate device IDs    different    testid    testid
    Execute Command    tedge cert remove

    ${output}=    Execute Command    tedge cert create c8y --device-id different
    Should Contain    ${output}    CN=different
    Validate device IDs    testid    different    testid
    Execute Command    tedge cert remove c8y

    ${output}=    Execute Command    tedge cert create c8y --device-id different --profile foo
    Should Contain    ${output}    CN=different
    Validate device IDs    testid    testid    different
    Execute Command    tedge cert remove c8y --profile foo

Input from --device-id is used over the values from tedge config settings when device.id and c8y.device.id are set
    Execute Command    tedge config set device.id testid
    Execute Command    tedge config set c8y.device.id c8y-testid
    Validate device IDs    testid    c8y-testid    testid

    ${output}=    Execute Command    tedge cert create c8y --device-id different
    Should Contain    ${output}    CN=different
    Validate device IDs    testid    different    testid
    Execute Command    tedge cert remove c8y

    ${output}=    Execute Command    tedge cert create c8y --device-id different --profile foo
    Should Contain    ${output}    CN=different
    Validate device IDs    testid    c8y-testid    different
    Execute Command    tedge cert remove c8y --profile foo

Input from --device-id is used over the values from tedge config settings when all device.id, c8y.device.id, c8y.profiles.foo.device.id are set
    Execute Command    tedge config set device.id testid
    Execute Command    tedge config set c8y.device.id c8y-testid
    Execute Command    tedge config set c8y.device.id c8y-foo-testid --profile foo
    Validate device IDs    testid    c8y-testid    c8y-foo-testid

    ${output}=    Execute Command    tedge cert create c8y --device-id different --profile foo
    Should Contain    ${output}    CN=different
    Validate device IDs    testid    c8y-testid    different

Generic device.id is used as "default" value if cloud profile doesn't have its own value when device.id is set
    Execute Command    tedge config set device.id testid
    Validate device IDs    testid    testid    testid

    ${output}=    Execute Command    tedge cert create
    Should Contain    ${output}    CN=testid
    Validate device IDs    testid    testid    testid

    ${output}=    Execute Command    tedge cert create c8y
    Should Contain    ${output}    CN=testid
    Validate device IDs    testid    testid    testid

    ${output}=    Execute Command    tedge cert create c8y --profile foo
    Should Contain    ${output}    CN=testid
    Validate device IDs    testid    testid    testid

Generic device.id is used as "default" value if cloud profile doesn't have its own value when device.id and c8y.device.id are set
    Execute Command    tedge config set device.id testid
    Execute Command    tedge config set c8y.device.id c8y-testid
    Validate device IDs    testid    c8y-testid    testid

    ${output}=    Execute Command    tedge cert create c8y
    Should Contain    ${output}    CN=c8y-testid
    Validate device IDs    testid    c8y-testid    testid

    ${output}=    Execute Command    tedge cert create c8y --profile foo
    Should Contain    ${output}    CN=testid
    Validate device IDs    testid    c8y-testid    testid

Generic device.id is used as "default" value if cloud profile doesn't have its own value when all device.id, c8y.device.id, c8y.profiles.foo.device.id are set
    Execute Command    tedge config set device.id testid
    Execute Command    tedge config set c8y.device.id c8y-testid
    Execute Command    tedge config set c8y.device.id c8y-foo-testid --profile foo
    Validate device IDs    testid    c8y-testid    c8y-foo-testid

    ${output}=    Execute Command    tedge cert create c8y --profile foo
    Should Contain    ${output}    CN=c8y-foo-testid
    Validate device IDs    testid    c8y-testid    c8y-foo-testid


*** Keywords ***
Validate device IDs
    [Arguments]    ${device_id}    ${c8y_device_id}    ${c8y_foo_device_id}
    ${output}=    Execute Command    tedge config get device.id    strip=${True}
    Should Be Equal    ${output}    ${device_id}
    ${output}=    Execute Command    tedge config get c8y.device.id    strip=${True}
    Should Be Equal    ${output}    ${c8y_device_id}
    ${output}=    Execute Command    tedge config get c8y.device.id --profile foo    strip=${True}
    Should Be Equal    ${output}    ${c8y_foo_device_id}

Custom Suite Setup
    Setup    register=${False}

Custom Test Setup
    Execute Command    tedge config set c8y.url example.com --profile foo
    Execute Command    tedge config set c8y.device.cert_path /etc/tedge/device-certs/tedge-certificate@default.pem
    Execute Command    tedge config set c8y.device.key_path /etc/tedge/device-certs/tedge-private-key@default.pem
    Execute Command
    ...    tedge config set c8y.device.cert_path --profile foo /etc/tedge/device-certs/tedge-certificate@foo.pem
    Execute Command
    ...    tedge config set c8y.device.key_path --profile foo /etc/tedge/device-certs/tedge-private-key@foo.pem

Custom Teardown
    Execute Command    tedge config unset device.id
    Execute Command    tedge config unset c8y.device.id
    Execute Command    tedge config unset c8y.device.id --profile foo
    Execute Command    tedge cert remove    ignore_exit_code=${True}
    Execute Command    tedge cert remove c8y    ignore_exit_code=${True}
    Execute Command    tedge cert remove c8y --profile foo    ignore_exit_code=${True}
