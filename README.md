# 🥁 Osu! Taiko Drum Controller

A cross-platform, DIY **Taiko drum controller** designed for playing **[osu!taiko](https://osu.ppy.sh/home)** on PC. This controller functions as a fully configurable keypad that detects vibration-based hits and maps them to keypress events, serving as a general-purpose input device.

The project includes:

- Hardware schematics and PCB layout  
- Firmware written in Rust using RTIC framework
- GUI interface for configuring the drum from the host side (TODO!)
- Jupyter notebook for simulating piezoelectric sensor signals and modeling post-processing, which is then rewritten within the firmware in no_std environment
- A command-line utility for configuring the drum from the host side.  
- 3D-printable components for building the physical enclosure

---
![Taiko Drum Controller Icon](./docs/drum_icon.png)
---

## Visual Overview

- **Assembled Physical Drum Controller**  
  ![Assembled Physical Representation](./docs/drum-image-made.png)

- **Electronic Schematic**  
  ![Schematic](./docs/sch.png)

- **PCB Layout**  
  ![PCB Layout](./docs/pcb.png)

---

### Project Overview

---

## Firmware

The firmware is written in Rust using the [RTIC framework](https://rtic.rs/), simulating a general-purpose HID device to ensure compatibility across all major operating systems. It simultaneously exposes a serial interface for configuration and control via utility software.

Each of the four piezoelectric sensors is sampled independently using dedicated ADC channels. The firmware captures both "Don" and "Kat" hits in pairs with minimal latency. Captured samples are fed into a queue and processed by a parser task that performs post-processing to detect real and spurious hits. Inner constants like `sensitivity` and `sharpness` are configurable from the utility software. Valid hits are mapped into keypresses and transmitted as USB HID reports.

All configuration data is stored in the last page of the flash memory and can be updated at runtime using the configuration utility.

---

## Hardware

The custom PCB is designed in KiCad and features core components typically found on “Blue Pill” development boards, including SWD debug headers and an onboard reset button. The controller is powered directly via USB, which also serves as the communication link for HID reports to the host system.

---

## Configuration Utility (TODO! swap to GUI utility)

A lightweight command-line utility written in Tcl is provided for runtime configuration. It allows to:

- Remap keypresses for each sensor. Can be changed to any proper keyboard key.
- Adjust hit detection `sensitivity` and `sharpness` to fine tune inner hit detection algorithm
- Send control commands, such as firmware reboot.
- Firmware update support (TODO!)

---

## 3D Printed Parts

All necessary 3D-printable components are located in the `3d/` directory. This includes:

- Raw Blender model files
- Cura-ready imported `.3mf` files
