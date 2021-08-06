## How to retrieve a JWT Token to authenticate on Cumulocity

## Overview

In order to [authenticate HTTP requests on Cumulocity](https://cumulocity.com/guides/10.5.0/reference/rest-implementation/#authentication),
a device can retrieve a JWT token using MQTT.

## Retrieving the token

Follow the below steps in order to retrieve the token from the Cumulocity cloud using MQTT.

Subscribe to `c8y/s/dat` topic

```
$ tedge mqtt sub c8y/s/dat --no-topic
```

Publish an empty message on `c8y/s/uat` topic

```
$ tedge mqtt pub c8y/s/uat ''
```

After a while the token will be published on the subscribed topic `c8y/s/dat` in the below format

71,[Base64 encoded JWT token]
