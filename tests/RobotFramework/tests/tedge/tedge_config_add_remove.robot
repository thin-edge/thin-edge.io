*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Logs

Test Tags           theme:cli    theme:configuration


*** Test Cases ***
tedge config add/remove value in empty array type config
    # Verify initial state
    ${initial}    Execute Command    tedge config list
    Should Contain    ${initial}    c8y.smartrest.templates=[]

    # Add value to empty array
    Execute Command    sudo tedge config add c8y.smartrest.templates 1
    ${add}    Execute Command    tedge config list
    Should Contain    ${add}    c8y.smartrest.templates=["1"]

    # Remove value
    Execute Command    sudo tedge config remove c8y.smartrest.templates 1
    ${remove}    Execute Command    tedge config list
    Should Contain    ${remove}    c8y.smartrest.templates=[]

tedge config add/remove value in non-empty array type config
    Execute Command    sudo tedge config set c8y.smartrest.templates 1
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    c8y.smartrest.templates=["1"]

    Execute Command    sudo tedge config add c8y.smartrest.templates 2,3
    ${add}    Execute Command    tedge config list
    Should Contain    ${add}    c8y.smartrest.templates=["1", "2", "3"]

    Execute Command    sudo tedge config remove c8y.smartrest.templates 2
    ${remove}    Execute Command    tedge config list
    Should Contain    ${remove}    c8y.smartrest.templates=["1", "3"]

    Execute Command    sudo tedge config remove c8y.smartrest.templates 1,3
    ${remove}    Execute Command    tedge config list
    Should Contain    ${remove}    c8y.smartrest.templates=[]

tedge config add/remove value in single value type config
    Execute Command    sudo tedge config add device.type changed-type
    ${add}    Execute Command    tedge config list
    Should Contain    ${add}    device.type=changed-type

    Execute Command    sudo tedge config remove device.type changed-type
    ${remove}    Execute Command    tedge config list
    Should Contain    ${remove}    device.type=thin-edge.io

tedge config skip duplicates
    Execute Command    sudo tedge config add c8y.smartrest.templates 1,2
    ${add}    Execute Command    tedge config list
    Should Contain    ${add}    c8y.smartrest.templates=["1", "2"]

    Execute Command    sudo tedge config add c8y.smartrest.templates 1,2
    ${add}    Execute Command    tedge config list
    Should Contain    ${add}    c8y.smartrest.templates=["1", "2"]

tedge config skip value to remove if it does not match
    Execute Command    sudo tedge config add c8y.smartrest.templates 1,2
    ${add}    Execute Command    tedge config list
    Should Contain    ${add}    c8y.smartrest.templates=["1", "2"]

    Execute Command    sudo tedge config remove c8y.smartrest.templates 3,4
    ${remove}    Execute Command    tedge config list
    Should Contain    ${remove}    c8y.smartrest.templates=["1", "2"]

tedge config append/remove default value
    # Verify initial state
    ${initial}    Execute Command    tedge config list
    Should Contain
    ...    ${initial}
    ...    az.topics=["te/+/+/+/+/m/+", "te/+/+/+/+/e/+", "te/+/+/+/+/a/+", "te/+/+/+/+/status/health"]

    # Add value to array with default values
    Execute Command    sudo tedge config add az.topics azure1,azure2
    ${add}    Execute Command    tedge config list
    Should Contain
    ...    ${add}
    ...    az.topics=["azure1", "azure2", "te/+/+/+/+/a/+", "te/+/+/+/+/e/+", "te/+/+/+/+/m/+", "te/+/+/+/+/status/health"]

    # Remove one of the default values and new value
    Execute Command    sudo tedge config remove az.topics azure2,te/+/+/+/+/status/health
    ${remove}    Execute Command    tedge config list
    Should Contain
    ...    ${remove}
    ...    az.topics=["azure1", "te/+/+/+/+/a/+", "te/+/+/+/+/e/+", "te/+/+/+/+/m/+"]
