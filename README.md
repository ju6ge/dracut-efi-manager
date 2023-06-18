Dracut-Stub-Manager
==================

## Why this Tool?

Well I am annoyed with the state of different boot-loaders for a while. Mostly because setting up Linux with an encrypted zfs root partition is not support by most or poorly documented. Instead I went a different route. 
Why use a boot-loader anyway. EFI is more than capable of handling booting the system. Just build your Linux kernel into a single EFI binary with included initramfs and add an efi boot entry to directly boot into the kernel.

Dracut supports this very well! All that is needed for it to be usable is to automate the process. I don't like shell scripts so I wrote this tool.

## Roadmap

- [ ] support building multiple efi-stubs at once
- [ ] working pacman hook
- [ ] mange efi boot entries automatically
