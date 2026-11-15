{ pkgs, wrosPackages }:
# Owner's acceptance gate (do not edit): the installed helper package must not name any Debian
# tool or data path. NixOS has no /usr/bin, /usr/sbin, /usr/libexec, /usr/lib, /usr/share, /sbin
# or /opt, and its /bin holds only sh. /bin/sh and /usr/bin/env do exist and are allowed.
# Paths inside the Nix store or under /run/wrappers and /run/current-system do not match.
pkgs.runCommand "wros-helpers-no-fhs" { } ''
  found=$(grep -rnIE '(^|[^A-Za-z0-9_./-])/(usr|sbin|bin|opt)/[A-Za-z0-9_.-]' ${wrosPackages.wros-helpers} \
    | grep -vE '(^|[^A-Za-z0-9_./-])(/bin/sh|/usr/bin/env)([^A-Za-z0-9_.-]|$)' || true)
  if [ -n "$found" ]; then
    echo "$found" | cut -c1-240 | head -200
    echo "wros-helpers still names $(echo "$found" | wc -l) Debian path(s)"
    exit 1
  fi
  touch "$out"
''
