from environment_c8y import EnvironmentC8y

"""
Run connection test while being connected (positive case):

Given a configured system with configured certificate
When we setup EnvironmentC8y
When we execute `sudo tedge connect c8y --test`
When we validate stdout
When we cleanup EnvironmentC8y
Then we find a successful message in stdout
Then the test has passed

"""


class TedgeConnectTestPositive(EnvironmentC8y):
    def execute(self):
        super().execute()
        self.systemctl = "/usr/bin/systemctl"
        self.log.info("Execute `tedge connect c8y --test`")
        self.tedge_connect_c8y_test()
        self.device_fragment = self.cumulocity.get_thin_edge_device_by_name(
            self.project.device
        )

    def validate(self):
        super().validate()
        self.log.info("Validate")
        self.assertGrep(
            "tedge_connect_c8y_test.out",
            "Connection check to c8y cloud is successful.",
            contains=True,
        )
        try:
            id = self.device_fragment["id"]
        except:
            self.log.error("Cannot find id in device_fragment")
            raise SystemError("Cannot find id in device_fragment")

        self.assertTrue(id != None, "thin-edge.io device with the given name exists")
