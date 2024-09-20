---
title: Benchmarks
sidebar_position: 10
---

# Benchmarks

This section contains performance benchmarks for ThinEdge.io, including detailed CPU and memory usage for key processes, measured on a device running in the OSADL QA Farm.

## Hardware Information

The following hardware and software specifications describe the environment in which these benchmarks are measured. The device runs ThinEdge.io and its associated plugins for data collection.

| Component      | Specification                                                 |
|----------------|---------------------------------------------------------------|
| **Processor**  | Quad-core ARM Cortex-A72 (ARM v8)                             |
| **Memory**     | 1.8 GB RAM                                                    |
| **Operating System** | Debian GNU/Linux 12 (bookworm)                          |
| **MQTT Broker**| Mosquitto (v2.0.18)                                           |
| **Firmware**   | ThinEdge.io (v1.1.2)                                          |
| **Plugins Used** | `tedgecpuprocent`, `memory`, `tedge-agent`, `tedge-mapper`  |

> The device is a Raspberry Pi 4 Model B Rev 1.5, a member of the OSADL QA Farm, which continuously collects data on performance and resource consumption.

## CPU Run-time Consumption of Key ThinEdge.io Tasks

In this section, we monitor the CPU usage of the most critical ThinEdge.io tasks: `mosquitto`, `tedge-agent`, and `tedge-mapper`. These tasks are essential for device-to-cloud communication, and their performance is crucial for the overall efficiency of the system.

### Graph of CPU Usage

Below is a visual representation of the CPU consumption by the main ThinEdge.io processes over the past month.

![CPU Usage](./tedgecpuprocent-month.svg)

For each process, the graph shows:
- **Command Name (comm)**: The name of the task running on the device.
- **CPU Usage (cuc)**: The percentage of CPU time utilized by each process.


The graph allows you to visually track CPU performance trends and identify potential bottlenecks in the system.

### Detailed Metrics

The following table provides a detailed description of the CPU consumption of measured tasks:

| Task                 | Description                                                    |
|----------------------|----------------------------------------------------------------|
| `mosquitto`          | Handles MQTT communications between devices and cloud          |
| `tedge-agent`        | Coordinates device management, software updates, and telemetry |
| `tedge-mapper_c8y`   | Converts telemetry data into the format required by cloud      |
| `tedge-mapper-colle` | Converts telemetry data into the format required               |

> Note: The values in this table are updated dynamically once per month.


## Memory Usage

In this section, we monitor the memory consumption of key system components to ensure efficient performance. The memory plugin tracks several metrics to provide a detailed view of how the system memory is being utilized.

### Graph of Memory Usage

Below is a visual representation of the memory consumption on the device over the past month.

![Memory Usage](./memory-month.svg)

### Monitored Metrics:
- **Total Memory**: The total amount of memory available on the system.
- **Free Memory**: Memory that is available for new processes.
- **Buffers**: Memory used by the system for temporary storage, primarily for block devices (like hard disks).
- **Cached Memory**: Memory that is cached for quicker access to frequently used data.
- **Swap Memory**: Disk space used as virtual memory when physical memory is full.
- **Slab Memory**: Memory used by the kernel for various internal data structures.
- **Shared Memory (shmem)**: Memory used for shared memory segments and tmpfs (temporary file system).
- **Swap Cache**: Memory that keeps track of pages that have been fetched from swap but not yet modified.
- **Page Tables**: Memory used to map virtual memory addresses to physical addresses.

For each metric, the graph shows the current and historical values, allowing you to monitor how memory is utilized by the system.

### Detailed Metrics

The following table provides a description of memory usage metrics:

| Metric           | Description                                                                 |
|------------------|-----------------------------------------------------------------------------|
| **Total Memory**  | The total amount of memory available on the system.                         |
| **Free Memory**   | Memory that is available for new processes.                                 |
| **Buffers**       | Memory used for temporary storage for block devices (e.g., hard disks).     |
| **Cached Memory** | Memory cached for quicker access to frequently used data.                   |
| **Swap Memory**   | Disk space used as virtual memory.                                          |
| **Slab Memory**   | Memory used by the kernel for internal data structures.                     |
| **Shared Memory** | Memory used for shared memory segments and tmpfs.                           |
| **Swap Cache**    | Memory keeping track of pages fetched from swap but not yet modified.       |
| **Page Tables**   | Memory used to map virtual memory addresses to physical memory addresses.   |

> The values in this table are updated dynamically once per month.

By monitoring these metrics, we can ensure that the system is using memory efficiently, and we can detect potential memory leaks or bottlenecks in the system's operation.

---

This section now contains all relevant CPU and memory metrics, helping you track and analyse how system resources are allocated and used over time. These benchmarks are crucial for maintaining overall system performance.
