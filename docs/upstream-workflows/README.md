# Preserved Grimoire application workflows

These unmodified upstream workflow sources retain their original attribution
and history. They are reference material outside `.github/workflows`, so GitHub
Actions does not execute them for WrOS:

- [Windows application checks](windows.yml)
- [Cross-platform application release](release.yml)
- [Application installer checks](installer-check.yml)
- [Application packages](packages.yml)

WrOS targets its Debian runtime and needs its own OS image, installation,
signing and recovery qualification. See [WrOS CI](../CI.md).
