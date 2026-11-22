{ pkgs, lib, version, wrosPackages }:
# A thin online update (rc23, #169): packages cache.nixos.org has are listed in the signed
# manifest, not shipped, and the laptop fetches them from there, checking each against the
# signature. Here a stand-in "system" (a small closure: WrOS's own part plus hello and jq) is
# bundled, thinned against a stand-in cache, and fetched by the real update engine from two local
# web servers (the channel, and the cache through /etc/wros/update-upstream-url). A tampered file
# on the cache is refused and nothing is staged.
let
  secretKeyFile = ./keys/test-signer.sec.asc;
  nextVersion = "${version}+thin";

  toplevel = pkgs.runCommand "wros-thin-test-system" { } ''
    mkdir -p "$out"
    echo "${pkgs.hello} ${pkgs.jq}" > "$out/refs"
  '';

  unsigned = import ../pkgs/update-bundle.nix {
    inherit pkgs lib;
    version = nextVersion;
    src = ../..;
    systemToplevel = toplevel;
  };

  # cache.nixos.org's stand-in: every package of the closure but the system itself, laid out as
  # `nix copy --to file://…` lays out a binary cache (as cache.nixos.org does).
  fakeCache = pkgs.runCommand "wros-thin-test-cache" { nativeBuildInputs = [ pkgs.python3 ]; } ''
    cp -r ${unsigned}/cache "$out"
    chmod -R u+w "$out"
    python3 - "$out" "${toplevel}" <<'EOF'
    import sys
    from pathlib import Path
    cache, top = Path(sys.argv[1]), sys.argv[2]
    info = cache / (Path(top).name.split('-', 1)[0] + '.narinfo')
    nar = [l[5:] for l in info.read_text().splitlines() if l.startswith('URL: ')][0]
    (cache / nar).unlink()
    info.unlink()
    EOF
  '';

  tamperedCache = pkgs.runCommand "wros-thin-test-cache-tampered" { } ''
    cp -r ${fakeCache} "$out"
    chmod -R u+w "$out"
    # Same size, one byte different: only the SHA-256 check can tell.
    for f in "$out"/nar/*; do printf X | dd of="$f" bs=1 count=1 conv=notrunc status=none; done
  '';

  thinBundle = pkgs.runCommand "wros-thin-test-bundle" {
    nativeBuildInputs = [ pkgs.gnupg pkgs.python3 pkgs.zstd ];
  } ''
    mkdir -p "$out" keep
    cp -r ${unsigned}/. "$out/"
    chmod -R u+w "$out"
    python3 - "$out" "${fakeCache}" keep <<'EOF'
    import hashlib, importlib.util, json, sys
    from datetime import datetime, timedelta, timezone
    from email.utils import format_datetime
    from pathlib import Path
    spec = importlib.util.spec_from_file_location('tool', '${../../tools/build-nixos-update-bundle.py}')
    tool = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(tool)
    out, cache, keep = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    manifest = json.loads((out / 'closure-manifest.json').read_text())
    manifest['cache_files'], manifest['thin'] = tool.thin_out(
        out, manifest['cache_files'], cache.as_uri() + '/', manifest['toplevel'], keep)
    assert manifest['thin'], 'nothing came from the stand-in cache'
    left = [f['path'] for f in manifest['cache_files'] if (out / f['path']).is_file()]
    assert len(left) <= 3, left  # nix-cache-info, and the system's .narinfo and archive
    raw = (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode()
    (out / 'closure-manifest.json').write_bytes(raw)
    now = datetime.now(timezone.utc).replace(microsecond=0)
    stamp = lambda moment: format_datetime(moment, usegmt=True).replace('GMT', 'UTC')
    (out / 'Release').write_text(
        'Origin: WrOS\nLabel: WrOS NixOS updates\nSuite: wros-nixos-updates\nCodename: nixos-26.05\n'
        f'Date: {stamp(now)}\nValid-Until: {stamp(now + timedelta(days=14))}\nArchitectures: amd64\n'
        f'SHA256:\n {hashlib.sha256(raw).hexdigest()} {len(raw)} closure-manifest.json\n',
        encoding='utf-8')
    EOF
    export GNUPGHOME="$TMPDIR/gnupg"
    mkdir -p -m 0700 "$GNUPGHOME"
    gpg --batch --import "${secretKeyFile}"
    fpr=$(gpg --batch --with-colons --fingerprint --list-secret-keys | awk -F: '/^fpr:/ { print $10; exit }')
    gpg --batch --yes --local-user "$fpr" --clearsign --output "$out/InRelease" "$out/Release"
  '';

  engine = "${wrosPackages.wros-helpers}/lib/wros-update/engine.py";
  fetch = cacheDir: ''
    python3 -I -c "
    import importlib.util, json, sys
    from pathlib import Path
    spec = importlib.util.spec_from_file_location('engine', '${engine}')
    e = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(e)
    got = e.fetch_online(base_url='http://127.0.0.1:8000/', cache=Path('${cacheDir}'))
    closure = e.parse_closure_manifest((Path(got['path']) / 'closure-manifest.json').read_bytes())
    e.import_verified_closure(got['path'], closure)
    print(json.dumps(got))
    "
  '';
in
pkgs.testers.runNixOSTest {
  name = "wros-thin-update";

  nodes.machine = { lib, ... }: {
    imports = [ ../modules/updates.nix ];
    _module.args = {
      inherit version;
      wrosPkgs = wrosPackages;
      wrosRoot = ../..;
      nixpkgsRev = "test";
      googleClients = null;
    };
    wros.updates.trustDir = ./keys;
    environment.etc."wros/version".text = "${version}\n";
    environment.etc."wros/update-upstream-url".text = "http://127.0.0.1:8001/\n";
    environment.systemPackages = [ pkgs.python3 ];
    virtualisation.memorySize = 1536;
  };

  testScript = ''
    import json
    machine.start()
    machine.wait_for_unit("multi-user.target")
    # Copied in, not store dependencies of the machine: those would register hello and jq.
    for name, path in (("channel", "${thinBundle}"), ("cache", "${fakeCache}"), ("tampered", "${tamperedCache}")):
        machine.copy_from_host(path, f"/srv/{name}")
    machine.succeed("systemd-run --unit channel -p WorkingDirectory=/srv/channel python3 -m http.server 8000 --bind 127.0.0.1")
    machine.succeed("systemd-run --unit cache -p WorkingDirectory=/srv/cache python3 -m http.server 8001 --bind 127.0.0.1")
    machine.wait_for_open_port(8000)
    machine.wait_for_open_port(8001)

    with subtest("a thin update fetches cache.nixos.org's packages and imports them"):
        machine.fail("nix-store --check-validity ${pkgs.hello}")
        got = json.loads(machine.succeed("""${fetch "/var/cache/wros-thin"}""").strip().splitlines()[-1])
        assert got["status"] == "fetched", got
        machine.succeed("nix-store --check-validity ${toplevel} ${pkgs.hello} ${pkgs.jq}")
        machine.succeed("journalctl -u cache | grep -q 'GET /nar/'")

    with subtest("a tampered file on the cache is refused and nothing is staged"):
        machine.succeed("systemctl stop cache")
        machine.succeed("systemd-run --unit cache2 -p WorkingDirectory=/srv/tampered python3 -m http.server 8001 --bind 127.0.0.1")
        machine.wait_for_open_port(8001)
        machine.succeed("nix-store --delete --ignore-liveness ${toplevel} ${pkgs.hello}")
        machine.fail("nix-store --check-validity ${pkgs.hello}")
        status, out = machine.execute("""(${fetch "/var/cache/wros-thin-2"}) 2>&1""")
        assert status != 0 and "SHA-256" in out, out
        machine.succeed("test -z \"$(ls -A /var/cache/wros-thin-2 | grep -v progress.json)\"")
  '';
}
