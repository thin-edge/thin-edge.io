# Connect

## Connect to Cumulocity IoT​

```shell
tedge config set c8y.url example.cumulocity.com​
```

Upload self-signed certificate, not needed in production with root cert!​

// Add comment why do we need to give password and user

```shell
$ tedge cert register c8y –-user <username>
Password:
```

Add happy commands output
Add known unhappy paths, permission issue, file exists ...

```shell
tedge connect c8y ​
```

Next steps: [Testing with MQTT pub and sub](./005_pub_sub.md)
