# Project Overview

## Vision
Thermos-Rust is a multi-zone thermostat system for hydronic heating, written in Rust. It's a safety-critical system that controls physical heating infrastructure with memory safety, reliability, and real-time constraints.

**Supported Platforms:**
- Linux (Raspberry Pi) - Full implementation with Redis and Prometheus
- ESP32-C6 (bare-metal) - Embedded no_std implementation with WiFi/MQTT

## Goals

### Completed - Linux (Raspberry Pi)
- [x] Multi-zone temperature control with independent setpoints
- [x] GPIO-based relay control for zone valves
- [x] DS18B20 1-Wire temperature sensor support (sysfs)
- [x] Redis state persistence for restart resilience
- [x] Prometheus metrics export for monitoring
- [x] Hysteresis control to prevent relay chatter

### Completed - ESP32-C6
- [x] Bare-metal no_std implementation with Embassy async
- [x] 1-Wire DS18B20 via RMT peripheral (hardware-timed)
- [x] I2C BME280 sensor support
- [x] NVS flash state persistence
- [x] WiFi connectivity
- [x] MQTT temperature logging

### Pending
- [ ] HTTP sensor integration for additional zones (Linux)
- [ ] Web UI for control and monitoring
- [ ] Matter/HomeKit integration via rs-matter (ESP32)

## Non-Goals
- **Not a general-purpose HVAC controller** - Optimized for hydronic heating only
- **Not cloud-connected** - Local control only for reliability and security
- **Not a thermostat replacement** - Works alongside existing thermostats

## Success Metrics
- **Reliability**: 99.9% uptime over 30 days
- **Safety**: Zero unsafe GPIO states, graceful degradation on errors
- **Performance**: <5% CPU usage, <10 MB memory on Raspberry Pi
- **Response time**: Sensor read + control decision in <6 seconds
- **Accuracy**: Temperature control within ±0.5°C of setpoint
