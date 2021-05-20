## Component responsibility




## Open questions

* How to return detailed errors from the plugin (as unknown version, missing dependency, conflicting version, ...)
* How to find a plugin?
   * It can be a combination of pre-registered plugins, configuration, $PATH search and directory search.
* How to list the plugins?
* How to manage process start / stop / restart *while* applying a list of updates.
* Is the `dm-agent` a mapper?
   * Sure, there is still a logic built around MQTT pub/sub,
   * But, this is far beyond message translation and mapping.
   * Security concerns + side effects + long operations
   * Need to run as root (to fork root processes).
* Each software update message has an id, to be used for the response ...
   * Do we have to report the result in one go ... or can we have several partial responses using the same id?
* When do we have to trigger post install action?
   * After a specific module?
   * Once installed all the modules of a given type?
   * All the modules?
   
### About plugin implementation
* When do we call `apt-get update` ? and `--auto-remove` ?
  * An idea is to add two commands `start` and `finalize` to the plugins to give them an opportunity for any update or clean actions.
* Do we support a way to call `apt-get upgrade` to update all the installed packages?
* Do we support a way to call `apt-get dist-upgrade`?
* `WARNING: apt does not have a stable CLI interface. Use with caution in scripts.`

