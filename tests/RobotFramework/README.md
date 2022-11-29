# How to access and use the Robotframework Tests

## 1. Connecting with OpenVPN
### 1.1. Installing OpenVPN Client
#### 1.1.1. Windows
1. [Installation guide for OpenVPN Connect Client on Windows](https://openvpn.net/vpn-server-resources/installation-guide-for-openvpn-connect-client-on-windows/)


#### 1.1.2. Linux (Debian and Ubuntu)

1. Type the following command into the Terminal: `sudo apt install apt-transport-https`. This is done to ensure that your apt supports the https transport. Enter the root password as prompted

2. Type the following command into the Terminal: `sudo wget https://swupdate.openvpn.net/repos/openvpn-repo-pkg-key.pub`. This will install the OpenVPN repository key used by the OpenVPN 3 Linux packages
3. Type the following command into the Terminal: `sudo apt-key add openvpn-repo-pkg-key.pub`
4. Type the following command into the Terminal: `sudo wget -O /etc/apt/sources.list.d/openvpn3.list https://swupdate.openvpn.net/community/openvpn3/repos/openvpn3-$DISTRO.list`. This will install the proper repository. Replace $DISTRO with the release name depending on your Debian/Ubuntu distribution (the table of release names for each distribution can be found below). In this case, focal is chosen since Ubuntu 20.04 is used
5. Type the following command into the Terminal: `sudo apt update`
6. Type the following command into the Terminal: `sudo apt install openvpn3`. This will finally install the OpenVPN 3 package

#### 1.1.3. MacOS
1. [Installation guide for OpenVPN Connect Client on macOS](https://openvpn.net/vpn-server-resources/installation-guide-for-openvpn-connect-client-on-macos/)

### 1.2. Add OpenVPN Profile
1. Request a Profile file (*.ovpn)
2. Open the OpenVPN Connect app and click plus.
3. Click **Browse** and locate the previously downloaded OpenVPN profile.
4. Select the profile in the file directory click **Open** in the file explorer.
5. Click **Add** to import the OpenVPN profile.

## 2. List of Devices available

1. Raspberry Pi 4 - 192.168.1.4	(user: pass:) to be used only for executing
2. Raspberry Pi 4 - 192.168.1.110 (user:pi pass:thinedge)
2. Raspberry Pi 3 - 192.168.1.120 (user:pi pass:thinedge)
3. Raspberry Pi 4 - 192.168.1.130 (user:pi pass:thinedge)
4. Raspberry Zero - 192.168.1.140 (user:zero pass:thinedge)
5. Raspberry PI 4 - 192.168.1.200 (user:pi pass:thinedge) - for NonFunctional tests

## 3. Run an .robot file


To run an robot file ssh to 192.168.1.4 and use the following command structure:

**PLEASE NOTE: This is example command**

`robot -d \results --timestampoutputs --log build_install_rpi.html --report NONE --variable BUILD:844 --variable HOST:192.168.1.130 /thin-edge.io-fork/tests/RobotFramework/tasks/build_install_rpi.robot`


## 4. List of Robot files
### 4.1. Task Automations
### **Installing specific SW Build**

File name: `build_install_rpi.robot`
What is this task automation doing:
1. Defining the Device ID, structure is (ST'timestamp') (eg. ST01092022091654)
2. Checking the architecture in order to download the right SW
3. Setting the file name for download
4. Checking if thinedge is already installed on device
	1. if not the selected build version will be downloaded and installed in following order
		1. Install Mosquitto
		2. Install Libmosquitto1
		3. Install Collectd-core
		4. thin-edge.io installation
		5. Install Tedge mapper
		6. Install Tedge agent
		7. Install tedge apt plugin
		8. Install Install c8y log plugin
		9. Install c8y configuration plugin
		10. Install Tedge watchdog
		11. Create self-signed certificate
		12. Set c8y URL
		13. Upload certificate
		14. Connect to c8y

	2. if yes than thin-edge.io will be uninstalled using the uninstallation script to purge it and than steps from 4.1 will take place

Example command to run the task:
robot -d \results --timestampoutputs --log health_tedge_mapper.html --report NONE --variable HOST:192.168.1.110 health_tedge_mapper.robot

## 5. Needed installation if own device is used
Following installation is needed in order to run the robot files on own device:

1. Python with PiP
	1. 	[https://www.python.org/downloads/](https://www.python.org/downloads/)
2. Robot Framework
	3. `pip install robotframework`
4. Browser Library
	1. Install node.js
		1. `sudo su`
		2. `curl -fsSL https://deb.nodesource.com/setup_17.x | bash -`
		3. `sudo apt-get install -y nodejs`
	2. Update pip `pip install -U pip` to ensure latest version is used
	3. Install robotframework-browser from the commandline: `pip install robotframework-browser`
	4. Install the node dependencies: run `rfbrowser init` in your shell
		1. if rfbrowser is not found, try `python -m Browser.entry init` or `python3 -m Browser.entry init`
10. SSHLibrary
	`pip install --upgrade robotframework-sshlibrary`
12. CryptoLibrary
	`pip install --upgrade robotframework-crypto`
14. MQTTLibrary
	`pip install robotframework-mqttlibrary`
16. Metrics
	`pip install robotframework-metrics==3.3.3`

## 6. Command line options for test execution

`-N, --name <name>`
	Sets the name of the top-level test suite.

`-D, --doc <document>`
	Sets the documentation of the top-level test suite.

`-M, --metadata <name:value>`
	Sets free metadata for the top level test suite.

`-G, --settag <tag>`
	Sets the tag(s) to all executed test cases.

`-t, --test <name>`
	Selects the test cases by name.

`-s, --suite <name>`
	Selects the test suites by name.

`-R, --rerunfailed <file>`
	Selects failed tests from an earlier output file to be re-executed.

`--runfailed <file>`
	Deprecated. Use --rerunfailed instead.

`-i, --include <tag>`
	Selects the test cases by tag.

`-e, --exclude <tag>`
	Selects the test cases by tag.

`-c, --critical <tag>`
	Tests that have the given tag are considered critical.

`-n, --noncritical <tag>`
	Tests that have the given tag are not critical.

`-v, --variable <name:value>`
	Sets individual variables.

`-V, --variablefile <path:args>`
	Sets variables using variable files.

`-d, --outputdir <dir>`
	Defines where to create output files.

`-o, --output <file>`
	Sets the path to the generated output file.

`-l, --log <file>`
	Sets the path to the generated log file.

`-r, --report <file>`
	Sets the path to the generated report file.

`-x, --xunit <file>`
	Sets the path to the generated xUnit compatible result file.

`--xunitfile <file>`
	Deprecated. Use --xunit instead.

`--xunitskipnoncritical`
	Mark non-critical tests on xUnit compatible result file as skipped.

`-b, --debugfile <file>`
	A debug file that is written during execution.

`-T, --timestampoutputs`
	Adds a timestamp to all output files.

`--splitlog`
	Split log file into smaller pieces that open in browser transparently.

`--logtitle <title>`
	Sets a title for the generated test log.

`--reporttitle <title>`
	Sets a title for the generated test report.

`--reportbackground <colors>`
	Sets background colors of the generated report.

`-L, --loglevel <level>`
	Sets the threshold level for logging. Optionally the default visible log level can be given separated with a colon (:).

`--suitestatlevel <level>`
	Defines how many levels to show in the Statistics by Suite table in outputs.

`--tagstatinclude <tag>`
	Includes only these tags in the Statistics by Tag table.

`--tagstatexclude <tag>`
	Excludes these tags from the Statistics by Tag table.

`--tagstatcombine <tags:title>`
	Creates combined statistics based on tags.

`--tagdoc <pattern:doc>`
	Adds documentation to the specified tags.

`--tagstatlink <pattern:link:title>`
	Adds external links to the Statistics by Tag table.

`--removekeywords <all|passed|name:pattern|for|wuks>`
	Removes keyword data from the generated log file.

`--flattenkeywords <name:pattern>`
	Flattens keywords in the generated log file.

`--listener <name:args>`
	Sets a listener for monitoring test execution.

`--warnonskippedfiles`
	Show a warning when an invalid file is skipped.

`--nostatusrc`
	Sets the return code to zero regardless of failures in test cases. Error codes are returned normally.

`--runemptysuite`
	Executes tests also if the selected test suites are empty.

`--dryrun`
	In the dry run mode tests are run without executing keywords originating from test libraries. Useful for validating test data syntax.

`--exitonfailure`
	Stops test execution if any critical test fails.

`--exitonerror`
	Stops test execution if any error occurs when parsing test data, importing libraries, and so on.

`--skipteardownonexit`
	Skips teardowns is test execution is prematurely stopped.

`--randomize <all|suites|tests|none>`
	Randomizes test execution order.

`--runmode <mode>`
	Deprecated in Robot Framework 2.8. Use separate --dryrun, --exitonfailure, --skipteardownonexit and --randomize options instead.

`-W, --monitorwidth <chars>`
	Sets the width of the console output.

`-C, --monitorcolors <on|off|force>`
	Specifies are colors used on the console.

`-K, --monitormarkers <on|off|force>`
 	Specifies are console markers (. and F) used.

`-P, --pythonpath <path>`
	Additional locations where to search test libraries from when they are imported.

`-E, --escape <what:with>`
	Escapes characters that are problematic in the console.

`-A, --argumentfile <path>`
	A text file to read more arguments from.

`-h, --help`
	Prints usage instructions.

`--version`
	Prints the version information.
