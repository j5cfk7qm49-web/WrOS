# GNU Unifont in WrOS

- Release: GNU Unifont 18.0.01 (signed 2026-09-16)
- File: `unifont-18.0.01.hex.gz`, unmodified, from
  https://unifoundry.com/pub/unifont/unifont-18.0.01/font-builds/unifont-18.0.01.hex.gz
- SHA-256: `e66385c79a0b8b24a466f3129930e08a966a935b4bf3b28c6bb17a9df9bf791d`
- Signature: `unifont-18.0.01.hex.gz.sig`, good signature from
  "Paul Hardy <unifoundry@unifoundry.com>", key
  `95D2 E9AB 8740 D804 6387 FD15 1A09 227B 1F43 5A33`, checked 2026-11-13.
- Debian 13's `fonts-unifont` is an older release; WrOS embeds its own copy,
  so no system package is needed.
- License: SIL Open Font License 1.1 (`UNIFONT-OFL-1.1.txt`, copied from the
  release tarball). Unifont is also offered under GPL 2 or later with the
  GNU font-embedding exception.

## WrOS changes

Unifont draws some symbols two cells wide where the layout gives them one.
`crates/wros-graphics/build.rs` fits every such symbol in U+2000-U+2BFF into
one cell when it builds the embedded table: scaled evenly to seven pixels wide
at the original's height, strokes carried by their thinned centre lines,
choosing among pixel alignments the result closest to the original that keeps
its strokes and holes. `wros-narrow.txt` holds hand drawings, in the frame of
Unifont's own one-cell symbols, for the 18 the interface itself uses. `crates/wros-graphics/build.rs`
replaces Unifont's drawing of each with WrOS's when it builds the embedded
table. The test `every_interface_symbol_is_as_wide_as_the_layout_gives_it`
fails if the interface starts using another such symbol.

Bold is drawn as the console's overstrike (each pixel and the one to its left)
and italic as a one-pixel lean above the middle of a lowercase letter, since
Unifont has one weight and no italic.

To update: download the new `font-builds/unifont-X.hex.gz` and its `.sig`,
verify the signature, replace the file, change the name in `build.rs`, and
re-run the width test.
