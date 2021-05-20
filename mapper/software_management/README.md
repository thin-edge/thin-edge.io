## Components

### Thin-Edge-Json

Responsibilities:
* Define the API between a cloud provider and the DM-agent
* Ability to convey as well:
   * desired software lists
   * software operation lists
   * device profile
   * current software lists
   * operation status
* Link a response to a request

Out of scope:
* Send the operation logs to the cloud.

Open questions:
* How to shorten the current software list sent by a device?

### Specific mapper - e.g. C8y Mapper

Responsibilities:
* Register the device, so it will receive the software management requests
* Listen and translate the operations received from the cloud.
* Forward these operations to the DM-agent and listen for the responses.
* Translate the DM-Agent responses and forward them to the cloud.

Open questions:
* Each software update message has an id, to be used for the response ...
   * Do we have to report the result in one go ... or can we have several partial responses using the same id?


### DM-Agent

Responsibilities:
* Listen the mappers for software management operations.
* Translate each software management request into a sequence of software management operations.
* Use a database to store the schedule operations and their status.
* Use the Plugin store to find the appropriate plugin for each operation.
* Apply the operation schedule, updating the database accordingly.

Out of scope:
* Manage the dependencies

### DM-Agent database

Responsibilities:
* Store the operations to schedule
* Store the result of each operation
  * One log file per install / uninstall command

Open questions:
* How to manage process start / stop / restart *while* applying a list of updates.
* Is the `dm-agent` a mapper?
  * Sure, there is still a logic built around MQTT pub/sub,
  * But, this is far beyond message translation and mapping.
  * Security concerns + side effects + long operations
  * Need to run as root (to fork processes with granted access to package installation).
* When do we have to trigger post install action?
   * After a specific module?
   * Once installed all the modules of a given type?
   * All the modules?

### Plugin API

Responsibilities:
* Define the interface between the DM-Agent and a plugin
* The operations (install / uninstall / list / version / ...) with their arguments
* Operation output (notably for list)
* Error status

Open questions:
* How to return detailed errors from the plugin (as unknown version, missing dependency, conflicting version, ...)
* Do we need more operations to trigger say updates, upgrades, cleanings ...?

### Plugin Store

Responsibilities:
* Find the appropriate plugin to install a software module.
* Define a default plugin to install software modules on the device.

Open questions:
* How to find a plugin?
   * It can be a combination of pre-registered plugins, configuration, $PATH search and directory search.
* How to list the plugins?

### Specific plugin - e.g. debian plugin

Responsibilities:
* Implement the plugin API for a specific kind of software modules.
* Manage the dependencies

Open questions:
* When do we call `apt-get update` ? and `--auto-remove` ?
  * An idea is to add two commands `start` and `finalize` to the plugins to give them an opportunity for any update or clean actions.
* Do we support a way to call `apt-get upgrade` to update all the installed packages?
* Do we support a way to call `apt-get dist-upgrade`?






   

