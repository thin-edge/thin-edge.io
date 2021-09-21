## Overview System Tests

### How to run the System-Tests

There is also similar Documentation at Archbee:

https://app.archbee.io/docs/9iGX1hbDjwAeMfyO9A3YE/2vkDj1wJ6LTct1_LnKBCm#m7-122-running-system-tests-manually-on-your-device-linux-pc


#### Running System tests manually on your device / Linux PC

The system tests can be executed manually when you set some environment variables in advance. Also compile the sawtooth_publisher in advance.

These enviroment variables need to be exported on your shell:

    export TEBASEDIR=~/thin-edge.io/
    export EXAMPLEDIR=$HOME/thin-edge.io/target/debug/examples
    export C8YUSERNAME= <your tenant>
    export C8YPASS= <your password>
    export C8YDEVICE= <dev id>
    export C8YTENANT= <tenant id>
    export C8YDEVICEID= <c8y dev id
    export C8YURL=<your url> e.g. : https://thin-edge-io.eu-latest.cumulocity.com



    ci/ci_run_all_tests.sh


    pysys.py run

You can selectively run tests based on their folder names:

    pysys.py run c8y_restart_bridge

With debugging enabled:

    pysys.py run -v DEBUG c8y_restart_bridge

Also with some wildcards:

    pysys.py run 'monitoring_*'


### Environments to derive tests from

Tests can be simplified by moving common parts to environments. Current environments are:

* `environment_az.py` : Environment to manage automated connect and disconnect to Microsoft Azure.

* `environment_c8y.py` : Environment to manage automated connect and disconnect to Cumulocity.

* `environment_roundtrip_c8y.py` : Environment to manage automated roundtrip tests for Cumulocity.



### Software Management End To End

See folder software-management-end-to-end

These tests are disabled by default as they will install and deinstall packages.
Better run them in a VM or a container.

To run the tests:

    pysys.py run 'sm-apt*' -XmyPlatform='smcontainer'

To run the tests with another tenant url:

    pysys.py run 'sm-apt*' -XmyPlatform='smcontainer' -Xtenant_url='thin-edge-io.eu-latest.cumulocity.com'

### Apt Plugin Tests

See folder apt_plugin

These tests are disabled by default as they will install and deinstall packages.
Better run them in a VM or a container.

To run the tests:

    pysys.py run 'apt_*' -XmyPlatform='container'



