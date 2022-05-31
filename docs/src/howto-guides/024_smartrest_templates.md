# How to use Cumulocity Custom SmartREST 2.0 Templates

[Custom SmartRest Templates](https://cumulocity.com/guides/reference/smartrest-two) can be used to extend the functionality of a device to support more operations than what the [static SmartREST templates](https://cumulocity.com/guides/reference/smartrest-two/#mqtt-static-templates) offer.

`thin-edge.io` supports subscription to custom templates as documented [here](https://cumulocity.com/guides/users-guide/device-management/#smartrest-templates).

For every template that the device uses, it must publish all data to `s/uc/<template-name>` topic and subscribe to `s/dc/<template-name>` to receive data from the cloud, based on that template.
When these templates are configured with `thin-edge.io`, subscriptions to all these relevant topics on Cumulocity cloud will be done by `thin-edge.io` internally.
Local processes on the device can access these templates on the local MQTT broker by simply publishing to `c8y/s/uc/<template-name>` and subscribing to `c8y/s/dc/<template-name>` topics (note the `c8y/` prefix in topics).

A template named `$TEMPLATE_NAME` requires the following subscriptions to be added when connecting to Cumulocity:

```plain
s/dc/$TEMPLATE_NAME
s/uc/$TEMPLATE_NAME
```

**This is not done automatically and the custom templates have to be declared using the `tedge` command.**

## Checking existing templates

```shell
tedge config get c8y.smartrest.templates
```

## Add new template to thin-edge configuration

To add new template to `thin-edge.io` the `tedge config` cli tool can be used as following:

```shell
tedge config set c8y.smartrest.templates template-1,template-2
```

> Note: To add/append a new template to a device that's already configured with some, all the existing templates should also be declared along with the new one in the `tedge config set` command.
> For example, if `template-1` is already configured on the device, as following:
>
> ```shell
> $ tedge config get c8y.smartrest.templates
> ["template-1"]
> ```
>
> To add new template to the set it is required to include current template, so the command would look like this:
>
> ```shell
> tedge config set c8y.smartrest.templates template-1,template-2
> ```
>
> Now when we get the configuration the both templates will be there:
>
> ```shell
> $ tedge config get c8y.smartrest.templates
> ["template-1", "template-2"]
> ```

## Removing templates from configuration

To remove all the templates, the `unset` subcommand can used as follows:

```shell
tedge config unset c8y.smartrest.templates
```

To remove one of existing templates you can overwrite the existing `c8y.smartrest.templates` with the new set which doesn't contain the unwanted template.

```shell
$ tedge config get c8y.smartrest.templates
["template-1", "template-2"]
```

```shell
tedge config set c8y.smartrest.templates template-1
```

```shell
$ tedge config get c8y.smartrest.templates
["template-1"]
```
