#### PUT - works (at least in PUT mode)

```sh
curl -XPOST --user "$C8Y_TENANT/${C8Y_USER}:${C8Y_PASSWORD}" -H "Accept: application/json" -H "X-Id: templateXIDexample11" -d '10,107,PUT,/inventory/managedObjects/%%,application/json,application/json,%%,UNSIGNED UNSIGNED,"{""value"":""%%""}"' "https://$(tedge config get c8y.http)/s"
```

```sh
tedge mqtt pub c8y/s/ul/templateXIDexample11 "$(printf '107,%s,%s' "9238352676" "1")"
```


### c8y-bridge.conf

```sh
topic s/ul/# out 2 c8y/ ""
topic t/ul/# out 2 c8y/ ""
topic q/ul/# out 2 c8y/ ""
topic c/ul/# out 2 c8y/ ""
topic s/dl/# in 2 c8y/ ""
topic s/ul/templateXIDexample10 out 2 c8y/ ""
topic s/dl/templateXIDexample10 in 2 c8y/ ""
topic s/ol/templateXIDexample10 in 2 c8y/ ""
```


### Problem with cert based device user when using SmartREST 1.0

```sh
[c8y/s/ul/templateXIDexample11] 107,9238352676,3333
[c8y/s/dl/templateXIDexample11] 50,1,401,Unauthorized
```
