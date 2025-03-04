*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Teardown      Get Logs
Test Setup          Custom Setup

Test Tags           theme:cli


*** Variables ***
${thing}=
...             SEPARATOR=
...             apt.name\tThe filtering criterion that is used to filter packages list output by name\n\
...             apt.maintainer\tThe filtering criterion that is used to filter packages list output by maintainer\n\
...             apt.dpk.options.config\tdpkg configuration option used to control the dpkg options "--force-confold" and "--force-confnew" and are applied when installing apt packages via the tedge-apt-plugin. Accepts either 'keepold' or 'keepnew'.


*** Test Cases ***
Tedge has completions for basic subcommands
    ${output}=    Execute Command    cmd=COMPLETE=fish tedge -- tedge con
    Should Be Equal
    ...    ${output}
    ...    config\tConfigure Thin Edge\n\connect\tConnect to cloud provider
    ...    strip_spaces=${True}

Tedge has completions for `tedge run`
    ${output}=    Execute Command    cmd=COMPLETE=fish tedge -- tedge run tedge-a
    Should Be Equal
    ...    ${output}
    ...    tedge-agent\ttedge-agent interacts with a Cloud Mapper and one or more Software Plugins\ntedge-apt-plugin\tThin-edge.io plugin for software management using apt
    ...    strip_spaces=${True}

Tedge has completions for configuration keys
    ${output}=    Execute Command    cmd=COMPLETE=fish tedge -- tedge config get apt
    Should Be Equal
    ...    ${output}
    ...    ${thing}
    ...    strip_spaces=${True}

Tedge has completions document MQTT QoS values
    ${output}=    Execute Command    cmd=COMPLETE=fish tedge -- tedge mqtt pub --qos ''
    Should Be Equal
    ...    ${output}
    ...    0\tAt most once\n1\tAt least once\n2\tExactly once
    ...    strip_spaces=${True}

Tedge has completions for cloud profile names
    # We can't see which cloud is selected when completing `connect <CLOUD>
    # --profile ''`, therefore we list profiles for all clouds
    Execute Command    cmd=tedge config set c8y.url --profile test test.example.com
    Execute Command    cmd=tedge config set c8y.url --profile other other.example.com
    Execute Command    cmd=tedge config set az.url --profile azure azure.example.com
    Execute Command    cmd=tedge config set aws.url --profile aws aws.example.com

    ${output}=    Execute Command    cmd=COMPLETE=fish tedge -- tedge connect c8y --profile '' | sort
    Should Be Equal
    ...    ${output}
    ...    aws\nazure\nother\ntest
    ...    strip_spaces=${True}


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup    skip_bootstrap=${True}
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect

    Set Test Variable    $DEVICE_SN    ${device_sn}
