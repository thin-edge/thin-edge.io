## Overview System Tests

System tests for thin-edge in this folder are written in Python with
the PySys system test framework.

See also:

* [https://pysys-test.github.io/pysys-test/](https://pysys-test.github.io/pysys-test/)
* [https://github.com/pysys-test/](https://github.com/pysys-test/)


#### Running System tests manually on your device / Linux PC

The system tests can be executed manually. In advance, you need a tenant in
Cumulocity as well as a previously generated device certificate. The device
needs to be able to connect to Cumulocity successfully.

The test configuration is located in environment variables, which need to be
set in in advance. Also compile the sawtooth_publisher, as it is used by some
tests.

    cargo build --example sawtooth_publisher

These environment variables need to be exported in your shell:

    export TEBASEDIR=~/thin-edge.io/
    export EXAMPLEDIR=$HOME/thin-edge.io/target/debug/examples
    export C8YUSERNAME= <your tenant>
    export C8YPASS= <your password>
    export C8YDEVICE= <dev name>
    export C8YTENANT= <tenant id>
    export C8YDEVICEID= <numeric c8y device id>
    export C8YURL=<your url> e.g. : https://thin-edge-io.eu-latest.cumulocity.com

Quickstart to run the tests:

    ci/ci_run_all_tests.sh

Run the tests in your own Python environment:

    python3 -m venv ~/env-pysys
    source ~/env-pysys/bin/activate
    pip3 install -r tests/requirements.txt
    cd tests/PySys/
    pysys.py run
    deactivate

You can selectively run tests based on their folder names:

    pysys.py run c8y_restart_bridge

With debugging enabled:

    pysys.py run -v DEBUG c8y_restart_bridge

Also with some wildcards:

    pysys.py run 'monitoring_*'


### Environments to derive tests from

Tests can be simplified by moving common parts to environments. Currently,
available environments are:

* `environment_az.py`:
    Environment to manage automated connect and disconnect to Microsoft Azure.

* `environment_c8y.py`:
    Environment to manage automated connect and disconnect to Cumulocity.

* `environment_roundtrip_c8y.py`:
    Environment to manage automated roundtrip tests for Cumulocity.


### Software Management End To End

See folder [software-management-end-to-end](/software-management-end-to-end/).

These tests are disabled by default as they will install and de-install packages.
Better run them in a VM or a container.

To run the tests:

    pysys.py run 'sm-apt*' -XmyPlatform='smcontainer'

To run the tests with another tenant url:

    pysys.py run 'sm-apt*' -XmyPlatform='smcontainer' -Xtenant_url='thin-edge-io.eu-latest.cumulocity.com'


### Apt Plugin Tests

See folder [apt_plugin](/apt_plugin/).

These tests are disabled by default as they will install and de-install packages.
Better run them in a VM or a container.

To run the tests:

    pysys.py run 'apt_*' -XmyPlatform='container'


Some of the tests desire a configured fakeplugin to simulate fruits.
To run the tests on a platform with fake plugin:

    pysys.py run -v DEBUG 'sm-fake*' -Xfakeplugin=fakeplugin -XmyPlatform=smcontainer

See file environment_sm_management.py on how to setup and configure the fake plugin.
