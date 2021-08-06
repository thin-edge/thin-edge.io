## How to retrieve the JWT Token

Once the Thin Edge device is connected to the Cumulocity cloud using the certificates, it can receive a token that
can be used later to authenticate HTTP requests.

To retrieve the token from the Cumulocity cloud follow the below steps.

Subscribe to `c8y/s/dat` topic

```
$ tedge mqtt sub c8y/s/dat --no-topic
```

Publish an empty message on `c8y/s/uat` topic

```
$ tedge mqtt pub c8y/s/uat ''
```

After a while the token will be published on the subscribed topic `c8y/s/dat` in the below format
71,<<Base64 encoded JWT token>>

Learn more about using JWT token [here](https://cumulocity.com/guides/10.6.0/reference/rest-implementation/)