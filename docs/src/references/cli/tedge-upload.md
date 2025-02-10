---
title: "tedge upload"
tags: [Reference, CLI]
sidebar_position: 7
---

# The tedge upload command

```sh title="tedge upload c8y"
Upload a file to Cumulocity

The command creates a new event for the device, attaches the given file content to this new event, and returns the event ID.

Usage: tedge upload c8y [OPTIONS] --file <FILE>

Options:
      --file <FILE>
          Path to the uploaded file

      --mime-type <MIME_TYPE>
          MIME type of the file content
          
          If not provided, the mime type is determined from the file extension
          If no rules apply, application/octet-stream is taken as a default

      --type <EVENT_TYPE>
          Type of the event
          
          [default: tedge_UploadedFile]

      --text <TEXT>
          Text description of the event. Defaults to "Uploaded file: <FILE>"

      --json <JSON>
          JSON fragment attached to the event
          
          [default: {}]

      --profile <PROFILE>
          Optional c8y cloud profile

      --device-id <DEVICE_ID>
          Cumulocity external id of the device/service on which the file has to be attached.
          
          If not given, the file is attached to the main device.

      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --debug
          Turn-on the DEBUG log level.
          
          If off only reports ERROR, WARN, and INFO, if on also reports DEBUG

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace.
          Logs with verbosity lower or equal to the selected level will be printed,
          i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
          
          Overrides `--debug`

  -h, --help
          Print help (see a summary with '-h')
```