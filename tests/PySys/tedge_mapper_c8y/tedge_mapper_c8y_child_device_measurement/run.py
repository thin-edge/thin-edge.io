from environment_c8y import EnvironmentC8y

import time
import json


class TedgeMapperC8yChildDevice(EnvironmentC8y):
    def setup(self):
        super.setup()
        child_device_fragment = self.cumulocity.get_child_device_of_thin_edge_device_by_name(
            self.project.device, "thin-edge-child")
        self.assertThat('actual == expected',
                        actual=child_device_fragment, expected=None)

    def execute(self):
        self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "pub",
                       "tedge/measurements/thin-edge-child", '{"temperature": 12, "time": "2021-06-15T17:01:15.806181503+02:00"}'],
            stdouterr="tedge_pub",
        )

        # Waiting for the mapped measurement message to reach the Cloud
        time.sleep(2)

        self.child_device_fragment = self.cumulocity.get_child_device_of_thin_edge_device_by_name(
            self.project.device, "thin-edge-child")

    def validate(self):
        measurement = self.cumulocity.get_last_measurements_from_device(
            self.child_device_fragment['id'])
        self.log.info(json.dumps(measurement, indent=4))

        self.assertThat('actual == expected',
                        actual=measurement['type'], expected='ThinEdgeMeasurement')
        self.assertThat('actual == expected',
                        actual=measurement['temperature']['temperature']['value'], expected=12)
        self.assertThat('actual == expected',
                        actual=measurement['time'], expected='2021-06-15T17:01:15.806181503+02:00')
