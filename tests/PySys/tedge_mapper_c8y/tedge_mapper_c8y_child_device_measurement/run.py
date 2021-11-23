from environment_c8y import EnvironmentC8y

import time
import json
import os

"""
Validate measurements published to child devices

When various measurements are sent to 2 different child devices and to the parent device,
Then validate that these measurements are associated to the appropriate devices in the cloud.
"""


class TedgeMapperC8yChildDevice(EnvironmentC8y):
    def setup(self):
        super().setup()
        child_device_fragment = self.cumulocity.get_child_device_of_thin_edge_device_by_name(
            self.project.device, "thin-edge-child")
        self.assertThat('actual == expected',
                        actual=child_device_fragment, expected=None)
        self.addCleanupFunction(self.test_cleanup)

    def execute(self):
        # Publish one temperature measurement to thin-edge-child device
        self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "pub",
                       "tedge/measurements/thin-edge-child",
                       '{"temperature": 12, "time": "2021-01-01T10:10:10.100+02:00"}'],
            stdouterr="tedge_pub",
            environs=os.environ
        )

        # Publish another temperature measurement to thin-edge-child device
        self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "pub",
                       "tedge/measurements/thin-edge-child",
                       '{"temperature": 11}'],
            stdouterr="tedge_pub",
            environs=os.environ
        )

        # Publish one temperature measurement to the parent thin-edge device
        self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "pub",
                       "tedge/measurements",
                       '{"temperature": 5}'],
            stdouterr="tedge_pub",
            environs=os.environ
        )

        # Publish one temperature measurement to the other child device
        self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "pub",
                       "tedge/measurements/other-thin-edge-child",
                       '{"temperature": 6}'],
            stdouterr="tedge_pub",
            environs=os.environ
        )

        # Publish another temperature measurement to thin-edge-child device
        self.startProcess(
            command=self.tedge,
            arguments=["mqtt", "pub",
                       "tedge/measurements/thin-edge-child",
                       '{"temperature": 10}'],
            stdouterr="tedge_pub",
            environs=os.environ
        )
        # Waiting for the mapped measurement message to reach the Cloud
        time.sleep(1)

        self.child_device_json = self.cumulocity.get_child_device_of_thin_edge_device_by_name(
            self.project.device, "thin-edge-child")
        self.other_child_device_json = self.cumulocity.get_child_device_of_thin_edge_device_by_name(
            self.project.device, "other-thin-edge-child")

    def validate(self):
        child_measurements = self.cumulocity.get_last_n_measurements_from_device(
            self.child_device_json['id'], 3)

        # Validate the last 3 measurements of thin-edge-child device
        self.validate_measurement(
            child_measurements[0], "temperature", 10, "2021-06-15T17:06:55.923+02:00")
        self.validate_measurement(
            child_measurements[1], "temperature", 11)
        self.validate_measurement(
            child_measurements[2], "temperature", 12)

        # Validate the last measurement of other-thin-edge-child device
        other_child_measurement = self.cumulocity.get_last_measurements_from_device(
            self.other_child_device_json['id'])
        self.validate_measurement(
            other_child_measurement, "temperature", 6)

        # Validate the last measurement of the parent thin-edge device
        parent_measurement = self.cumulocity.get_last_measurements_from_device(
            self.project.deviceid)
        self.validate_measurement(
            parent_measurement, "temperature", 5)

    def validate_measurement(self, measurement_json,
                             measurement_key, measurement_value,
                             measurement_timestamp=None,
                             measurement_type="ThinEdgeMeasurement"):
        self.log.info(json.dumps(measurement_json, indent=4))
        self.assertThat('actual == expected',
                        actual=measurement_json['type'], expected=measurement_type)
        self.assertThat('actual == expected',
                        actual=measurement_json[measurement_key][measurement_key]['value'], expected=measurement_value)
        if measurement_timestamp:
            self.assertThat('actual == expected',
                            actual=measurement_json['time'], expected=measurement_timestamp)

    def test_cleanup(self):
        self.cumulocity.delete_managed_object_by_internal_id(
            self.child_device_json['id'])
        self.cumulocity.delete_managed_object_by_internal_id(
            self.other_child_device_json['id'])
