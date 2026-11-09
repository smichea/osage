// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! The machine the bridge runs on, read from Linux sysfs: CPU package
//! temperature and battery level. Read-only; nothing is ever written.

use std::{fs, path::PathBuf};

use osage_sdk::components::{Device, Measurement};

use crate::{Sensor, measured_now, read_only};

/// Thermal zone types worth reporting as the CPU temperature, preferred first.
const CPU_ZONES: &[&str] = &["x86_pkg_temp", "TCPU", "cpu-thermal", "acpitz"];

pub struct ThisMachine {
    cpu_zone: Option<PathBuf>,
    battery: Option<PathBuf>,
}

impl ThisMachine {
    pub fn detect() -> Self {
        Self::detect_in("/sys/class")
    }

    /// Looks for sensors under `sysfs_class` (e.g. a fixture directory in tests).
    pub fn detect_in(sysfs_class: &str) -> Self {
        let entries = |dir: &str| fs::read_dir(format!("{sysfs_class}/{dir}")).into_iter().flatten().flatten();
        let zone_type = |path: &PathBuf| fs::read_to_string(path.join("type")).unwrap_or_default().trim().to_owned();
        let zones: Vec<PathBuf> = entries("thermal").map(|e| e.path()).collect();
        let cpu_zone = CPU_ZONES.iter().find_map(|wanted| zones.iter().find(|z| zone_type(z) == *wanted).cloned());
        let battery = entries("power_supply")
            .map(|e| e.path())
            .find(|p| fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Battery"));
        Self { cpu_zone, battery }
    }
}

fn read_number(path: PathBuf) -> Option<f64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

impl Sensor for ThisMachine {
    fn local_id(&self) -> &str {
        "this-machine"
    }

    fn describe(&self) -> Device {
        let mut functions = Vec::new();
        if self.cpu_zone.is_some() {
            functions.push(read_only("cpu_temperature"));
        }
        if self.battery.is_some() {
            functions.push(read_only("battery_level"));
        }
        let name = fs::read_to_string("/sys/class/dmi/id/product_version")
            .or_else(|_| fs::read_to_string("/sys/class/dmi/id/product_name"))
            .map_or_else(|_| "this machine".to_owned(), |n| n.trim().to_owned());
        Device { name, functions, ..Default::default() }
    }

    fn read(&mut self) -> Vec<Measurement> {
        let mut readings = Vec::new();
        if let Some(millidegrees) = self.cpu_zone.clone().and_then(|z| read_number(z.join("temp"))) {
            readings.push(measured_now("cpu_temperature", millidegrees / 1000.0, "Cel"));
        }
        if let Some(percent) = self.battery.clone().and_then(|b| read_number(b.join("capacity"))) {
            readings.push(measured_now("battery_level", percent, "%"));
        }
        readings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_preferred_cpu_zone_and_the_battery_from_sysfs() {
        let root = std::env::temp_dir().join(format!("osage-sysfs-{}", std::process::id()));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write("thermal/thermal_zone0/type", "acpitz\n");
        write("thermal/thermal_zone0/temp", "40000\n");
        write("thermal/thermal_zone9/type", "x86_pkg_temp\n");
        write("thermal/thermal_zone9/temp", "55500\n");
        write("power_supply/AC/type", "Mains\n");
        write("power_supply/BAT0/type", "Battery\n");
        write("power_supply/BAT0/capacity", "87\n");

        let mut machine = ThisMachine::detect_in(root.to_str().unwrap());
        let readings = machine.read();
        let value = |name: &str| readings.iter().find(|m| m.name == name).map(|m| (m.value, m.unit.clone()));
        assert_eq!(value("cpu_temperature"), Some((55.5, "Cel".to_owned())));
        assert_eq!(value("battery_level"), Some((87.0, "%".to_owned())));
        assert!(readings.iter().all(|m| m.source_time_us > 0));
        fs::remove_dir_all(root).unwrap();
    }
}
