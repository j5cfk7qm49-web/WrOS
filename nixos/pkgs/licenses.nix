{ pkgs, lib, version, src }:
pkgs.stdenv.mkDerivation {
  pname = "wros-licenses";
  inherit version;
  src = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions [
      (src + "/LICENSE")
      (src + "/packaging/copyright")
      (src + "/assets/DEJAVU-LICENSE")
      (src + "/assets/dict/DICTIONARY-LICENSE")
      (src + "/assets/fonts/UNIFONT-OFL-1.1.txt")
      (src + "/assets/fonts/UNIFONT-SOURCE.md")
      (src + "/docs")
      (src + "/os/licenses")
    ];
  };

  buildPhase = ''
    runHook preBuild
    mkdir -p "$out/share/doc/wros/docs" "$out/share/doc/wros/os/licenses" "$out/share/doc/wros/upstream/os/licenses/upstream" "$out/share/doc/wros/python/os/licenses"
    install -m 644 LICENSE "$out/share/doc/wros/LICENSE-grimoire"
    install -m 644 packaging/copyright "$out/share/doc/wros/copyright"
    install -m 644 assets/DEJAVU-LICENSE "$out/share/doc/wros/DEJAVU-LICENSE"
    install -m 644 assets/dict/DICTIONARY-LICENSE "$out/share/doc/wros/DICTIONARY-LICENSE"
    install -m 644 assets/fonts/UNIFONT-OFL-1.1.txt "$out/share/doc/wros/UNIFONT-OFL-1.1.txt"
    install -m 644 assets/fonts/UNIFONT-SOURCE.md "$out/share/doc/wros/UNIFONT-SOURCE.md"
    install -m 644 docs/*.md "$out/share/doc/wros/docs/"
    install -m 644 docs/*licenses.json "$out/share/doc/wros/docs/"
    install -m 644 docs/feature-status.json "$out/share/doc/wros/feature-status.json"
    install -m 644 docs/keyboard-reference.html "$out/share/doc/wros/keyboard-reference.html"
    install -m 644 docs/upstream-notice-sources.json "$out/share/doc/wros/upstream-notice-sources.json"
    install -m 644 docs/python-tools-lock.json "$out/share/doc/wros/python-tools-lock.json"
    install -m 644 docs/python-tool-notices.json "$out/share/doc/wros/python-tool-notices.json"
    install -m 644 os/licenses/*.txt "$out/share/doc/wros/os/licenses/"
    install -m 644 os/licenses/upstream/*.txt "$out/share/doc/wros/upstream/os/licenses/upstream/"
    cp -r os/licenses/python "$out/share/doc/wros/python/os/licenses/python"
    runHook postBuild
  '';

  dontInstall = true;
}
