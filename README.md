# DMA HWID Spoofer

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-Windows%2010%20%2F%2011-blue.svg)
![Stars](https://img.shields.io/github/stars/vibheksoni/dma-spoofer?style=social)
![Forks](https://img.shields.io/github/forks/vibheksoni/dma-spoofer?style=social)

> The first open-source DMA-based HWID spoofer implementation.

This project is not another usermode cleaner. It is not another basic kernel driver. It is a real DMA-first HWID spoofing implementation built around direct memory access, build-aware reverse engineering, and consistency across the hardware identifier surfaces anti-cheats actually correlate.

If you understand what that means, then you already understand why this project matters.

## Why This Is Different

Every public project in this space tends to fall into one of two categories:

- usermode tools that patch easy registry or API-visible identifiers
- kernel drivers that cover one or two IOCTL paths but leave the rest of the machine state inconsistent

This repository exists to push past that.

The goal here is to build the first DMA-based implementation that actually works end-to-end by treating identity spoofing as a consistency problem across:

- live kernel query paths
- cached kernel structures
- registry mirrors
- storage and volume metadata
- firmware and boot state
- network and display side channels

That is the difference between a toy spoofer and a serious implementation.

## What It Covers

Current spoofer modules:

- SMBIOS
- Disk
- NIC
- GPU
- Monitor
- Volume
- Registry
- TPM
- EFI
- Boot
- ARP
- USB

Support systems included in the codebase:

- DMA access engine
- signature scanning
- code cave helpers
- registry hive parsing
- HWID generation helpers
- DSE research
- PatchGuard research

## Repository Stats

Current tracked source footprint:

- 12 spoofing modules
- 77 tracked source files under `src/`
- 12,106 tracked source lines under `src/`

## Current Focus

This is an active reverse-engineering project with runtime validation on real targets.

Recent build-aware work includes:

- Win10 SMBIOS support
- Win10 volume GUID support
- Win10 EFI support
- Win10 ARP support using the real `Ipv4Global` root
- Win10 dxgkrnl EDID cache resolution via `DXGGLOBAL`
- Win11 ARP layout alignment
- Win11 dxgkrnl EDID cache layout alignment

The important point is not that offsets exist. The important point is that the implementation is being aligned to the actual structures Windows uses on live targets, not just static samples.

## Architecture

```text
src/
|-- main.rs
|-- cli.rs
|-- core/
|   |-- dma.rs
|   |-- dse/
|   `-- patchguard/
|-- hwid/
|   |-- generator.rs
|   |-- manufacturers.rs
|   |-- oui.rs
|   `-- patterns.rs
|-- spoofers/
|   |-- arp/
|   |-- boot/
|   |-- disk/
|   |-- efi/
|   |-- gpu/
|   |-- monitor/
|   |-- nic/
|   |-- registry/
|   |-- smbios/
|   |-- tpm/
|   |-- usb/
|   `-- volume/
`-- utils/
    |-- codecave.rs
    |-- random.rs
    |-- registry.rs
    `-- signature.rs
```

## Requirements

- Windows 10 or Windows 11
- Rust toolchain
- Python 3
- administrator shell
- MemProcFS / LeechCore runtime DLLs

Primary development and validation path:

- Windows
- PowerShell
- MemProcFS
- VMware target VMs

## Required DLLs

The executable expects the runtime DLLs next to the built binary.

Required sources:

- `vmm.dll` from MemProcFS
- `leechcore.dll` from MemProcFS
- `vmmyara.dll` from MemProcFS
- `FTD3XX.dll` from FTDI D3XX drivers
- `FTD3XXWU.dll` from FTDI D3XX drivers

Download links:

- MemProcFS releases: [https://github.com/ufrisk/MemProcFS/releases](https://github.com/ufrisk/MemProcFS/releases)
- FTDI D3XX drivers: [https://ftdichip.com/drivers/d3xx-drivers/](https://ftdichip.com/drivers/d3xx-drivers/)

Typical placement:

- next to `dma-spoofer.exe`

## Build

```powershell
git clone https://github.com/vibheksoni/dma-spoofer.git
cd dma-spoofer
cargo build --release
```

Output:

- `target\release\dma-spoofer.exe`

## Usage

Examples:

```powershell
.\target\release\dma-spoofer.exe -v --test-all
.\target\release\dma-spoofer.exe -v --list all
.\target\release\dma-spoofer.exe -v --list smbios
.\target\release\dma-spoofer.exe -v --list arp
.\target\release\dma-spoofer.exe -v --list dxgkrnl
```

## Module Coverage

| Module | Focus |
| --- | --- |
| `smbios` | firmware tables and system identity |
| `disk` | storage serials and driver-backed disk identity |
| `nic` | MAC identity and cached adapter state |
| `gpu` | NVIDIA UUID and related GPU identifiers |
| `monitor` | registry EDID and dxgkrnl EDID cache |
| `volume` | volume GUIDs and related mount metadata |
| `registry` | tracked registry traces and cleanup |
| `tpm` | TPM-related traces and hooks |
| `efi` | EFI runtime variable surfaces |
| `boot` | boot identifiers and related state |
| `arp` | neighbor and ARP cache identity leakage |
| `usb` | removable-device identity surfaces |

## Design Principles

- Prefer real kernel-backed identity sources over shallow usermode mirrors.
- Treat spoofing as a build-aware problem.
- Keep registry-visible state aligned with the underlying kernel state whenever possible.
- Validate against live targets, not only static IDA samples.
- Build toward whole-machine identifier consistency, not one-off field edits.

## What Is Still Missing

High-value areas still worth covering:

- ACPI firmware identity
- WMI and CIM mirrors
- PCI and PnP registry identity
- display persistence stores beyond raw EDID and dxgkrnl cache
- CPU and hypervisor identity
- network profile and NLA persistence
- additional storage registry mirrors
- device container IDs and related grouping identifiers

## Stability

This project modifies live kernel state.

Higher-risk areas include:

- PatchGuard-related work
- DSE-related work
- EFI runtime tampering
- TPM-related low-level changes

Use a VM first.

## Contributing

If you reverse new layouts, validate additional Windows builds, extend spoofing coverage, or improve consistency across identifier surfaces, contributions are welcome.

Good contributions include:

- new Windows build support
- better runtime validation
- reversing missing hardware identity paths
- improving consistency between kernel state and registry-visible state
- better detection and recovery for unstable targets

If you want to contribute, open an issue or send a pull request.

## Support The Project

If you respect the engineering work behind this repository:

- star the project
- share it with people doing serious anti-cheat and low-level Windows research
- contribute code, reversing notes, or validation results

That helps turn this from a one-off release into the reference implementation for DMA-first HWID spoofing research.

## Disclaimer

This repository is for research and educational use.

If you run it, you accept responsibility for:

- legal compliance
- target-system safety
- third-party terms and policy issues
- crashes, corruption, boot problems, and data loss

Use it on systems you can afford to break.

## License

MIT

## Author

[vibheksoni](https://github.com/vibheksoni)

- X/Twitter: [@ImVibhek](https://x.com/ImVibhek)
- Website: [vibheksoni.com](https://vibheksoni.com/)
- GitHub: [vibheksoni](https://github.com/vibheksoni)
