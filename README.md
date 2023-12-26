Dracut-Stub-Manager
==================

## Why this Tool?

Well I am annoyed with the state of different boot-loaders for a while. Mostly because setting up Linux with an encrypted zfs root partition is not supported by most or is poorly documented. Instead I went a different route. 
Why use a boot-loader anyway. EFI is more than capable of handling booting the system. Just build your Linux kernel into a single EFI binary with included initramfs and add an efi boot entry to directly boot into the kernel.

Dracut supports this well! All that is needed for it to be usable is to automate the process. I don't like shell scripts so I wrote this tool.


## Settings

``` toml
kernel_modules_dir = "/usr/lib/modules"
efi_dir = "/boot/efi"

[build_mappings]
lts = "ArchLinuxLtsZfsStub.efi"
zen = "ArchLinuxZfsStub.efi"
```

## Roadmap
- [x] stub generation
- [x] working pacman hook
- [x] mange efi boot entries
- [ ] sign efi images for secure boot
- [ ] support building multiple efi binaries at once
