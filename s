#!/bin/sh
# WrOS live session: let the developer's PC in over SSH, to debug the installer.
# Key login only, from the local network, as root, until the next restart (all of it is in RAM).
echo "== WrOS debug SSH"
S=$(ls -d /nix/store/*-openssh-[0-9]* 2>/dev/null | grep -v -e '-man$' -e '-dev$' | head -1)
if [ ! -x "$S/bin/sshd" ]; then echo "STOP: no sshd found"; exit 1; fi
echo "sshd: $S"
D=/run/wros-debug-ssh
mkdir -p $D && chmod 700 $D
echo 'from="192.168.*,10.*,172.*" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIKODEJ1wfGL3+Q7mR8sqPHwhIDsLc7Rd09kuQ3AKwrBl claude-code-to-wros-laptop' > $D/authorized_keys
chmod 600 $D/authorized_keys
grep -q '^sshd:' /etc/group || echo 'sshd:x:74:' >> /etc/group
grep -q '^sshd:' /etc/passwd || echo 'sshd:x:74:74:SSH privilege separation:/var/empty:/run/current-system/sw/bin/nologin' >> /etc/passwd
mkdir -p /var/empty
# A locked root ("!") refuses key logins when PAM is off; "*" still has no usable password.
sed -i 's/^root:![^:]*:/root:*:/' /etc/shadow
[ -f $D/host_ed25519 ] || "$S/bin/ssh-keygen" -q -t ed25519 -N '' -f $D/host_ed25519
if "$S/bin/sshd" -f /dev/null -h $D/host_ed25519 -p 22     -o PermitRootLogin=prohibit-password -o PasswordAuthentication=no     -o KbdInteractiveAuthentication=no -o AuthorizedKeysFile=$D/authorized_keys     -o PidFile=$D/sshd.pid; then
  echo "sshd: started"
else
  echo "sshd: FAILED"
fi
if iptables -I nixos-fw 1 -p tcp --dport 22 -j nixos-fw-accept 2>/dev/null; then
  echo "firewall: opened (iptables)"
elif nft insert rule inet nixos-fw input-allow tcp dport 22 accept 2>/dev/null; then
  echo "firewall: opened (nftables)"
else
  echo "firewall: could not open port 22"
fi
echo "addresses:"
ip -4 -o addr show scope global | awk '{print "  " $2 " " $4}'
