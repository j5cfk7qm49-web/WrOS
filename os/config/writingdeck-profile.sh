# Installed in /etc/profile.d by appliance mode only, after an actual PAM login.
if [ "$(id -u)" -ne 0 ] && [ "$(tty 2>/dev/null)" = /dev/tty1 ] && id -nG | grep -qw writingdeck; then
    exec /usr/libexec/writingdeck-session
fi
