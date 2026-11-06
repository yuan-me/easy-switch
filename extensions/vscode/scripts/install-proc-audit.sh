#!/bin/sh
# Installs a root-owned, read-only checker; sudo grants no arguments or switching rights.
set -eu
[ "$(id -u)" = 0 ] || { echo '请通过 sudo sh 运行此安装脚本。' >&2; exit 1; }
case "${SUDO_USER:-}" in ''|*[!a-zA-Z0-9_-]*) echo '无法确认原始 sudo 用户。' >&2; exit 1;; esac
[ "$(id -u -- "$SUDO_USER")" = "${SUDO_UID:-}" ] && [ "$SUDO_UID" -gt 0 ] || exit 1
command -v visudo >/dev/null
/usr/bin/python3 -I -B -c 'import sys; assert sys.version_info >= (3, 9)'
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
destination=/usr/local/lib/easy-switch
for directory in /usr /usr/local /usr/local/lib /etc /etc/sudoers.d; do
    [ ! -L "$directory" ] && [ -d "$directory" ] && [ "$(stat -c %u "$directory")" = 0 ] || exit 1
    [ "$(find "$directory" -maxdepth 0 -perm /022 -print)" = '' ] || exit 1
done
if [ -e "$destination" ] || [ -L "$destination" ]; then
    [ ! -L "$destination" ] && [ -d "$destination" ] && [ "$(stat -c %u "$destination")" = 0 ] || exit 1
    [ "$(find "$destination" -maxdepth 0 -perm /022 -print)" = '' ] || exit 1
else
    install -d -o root -g root -m 0755 "$destination"
fi
for target in "$destination/proc-audit.py" "/etc/sudoers.d/easy-switch-proc-audit-$SUDO_UID"; do
    if [ -e "$target" ] || [ -L "$target" ]; then
        [ ! -L "$target" ] && [ -f "$target" ] && [ "$(stat -c %u "$target")" = 0 ] || exit 1
        [ "$(find "$target" -maxdepth 0 -perm /022 -print)" = '' ] || exit 1
    fi
done
script_tmp=$(mktemp "$destination/.audit.XXXXXX")
rule_tmp=$(mktemp /etc/sudoers.d/.easy-switch.XXXXXX)
trap 'rm -f -- "$script_tmp" "$rule_tmp"' EXIT HUP INT TERM
install -o root -g root -m 0644 "$source_dir/proc-audit.py" "$script_tmp"
printf '%s ALL=(root) NOPASSWD: /usr/bin/python3 -I -B /usr/local/lib/easy-switch/proc-audit.py\n' "$SUDO_USER" > "$rule_tmp"
chmod 0440 "$rule_tmp"
visudo -cf "$rule_tmp"
mv -f -- "$script_tmp" "$destination/proc-audit.py"
mv -f -- "$rule_tmp" "/etc/sudoers.d/easy-switch-proc-audit-$SUDO_UID"
echo '只读进程检查器已安装；未修改 Codex 配置或聊天。'
