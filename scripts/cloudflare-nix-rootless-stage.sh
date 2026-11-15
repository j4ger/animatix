#!/usr/bin/env bash
set -euo pipefail
[ "$#" = 6 ] || { echo 'BLOCKED: expected isolated-root arguments' >&2; exit 2; }
root=$1
repo=$2
stage=$3
parent_userns=$4
parent_mntns=$5
host_uid=$6
# Refuse direct invocation in the original namespaces, before any mount call.
[[ "$host_uid" =~ ^[1-9][0-9]*$ ]] || { echo 'BLOCKED: non-root host UID required' >&2; exit 2; }
[ "$(id -u)" = 0 ] &&
  [ "$(readlink /proc/self/ns/user)" != "$parent_userns" ] &&
  [ "$(readlink /proc/self/ns/mnt)" != "$parent_mntns" ] &&
  awk -v uid="$host_uid" 'NR==1 {ok=($1==0 && $2==uid && $3==1)} END {exit !(ok && NR==1)}' /proc/self/uid_map ||
  { echo 'BLOCKED: isolated rootless namespaces required' >&2; exit 2; }
[[ "$root" = /* && "$repo" = /* && "$root" != / && "$root" != "$repo" ]] &&
  [ "$(realpath -e "$root")" = "$root" ] &&
  [ "$(realpath -e "$repo")" = "$repo" ] &&
  [ "$(basename "$root")" = root ] &&
  [[ "$(basename "$(dirname "$root")")" = animatix-nix-probe-* ]] &&
  [ -d "$root/nix/store" ] && [ -f "$root/nix/bootstrap-reginfo" ] &&
  [ -f "$repo/flake.lock" ] || { echo 'BLOCKED: invalid temporary root or checkout' >&2; exit 2; }
# These mounts affect only this freshly isolated mount namespace.
mount --make-rprivate /
for item in usr bin sbin lib lib64; do
  [ ! -e "/$item" ] && continue
  mkdir -p "$root/$item"
  mount --bind "/$item" "$root/$item"
  mount -o remount,bind,ro "$root/$item"
done
# Do not bind the host's /etc: some builders lock its mount flags. Supply only
# the resolver and public CA bundle; never copy host accounts or credentials.
mkdir -p "$root/etc/ssl/certs"
cp /etc/resolv.conf "$root/etc/resolv.conf"
cp /etc/ssl/certs/ca-certificates.crt "$root/etc/ssl/certs/ca-certificates.crt"
printf 'root:x:0:0:Namespace root:/home/nix:/bin/bash\n' > "$root/etc/passwd"
printf 'root:x:0:\n' > "$root/etc/group"
printf 'passwd: files\ngroup: files\nhosts: files dns\n' > "$root/etc/nsswitch.conf"
printf '127.0.0.1 localhost\n::1 localhost\n' > "$root/etc/hosts"
mkdir -p "$root/dev" "$root/proc"
for item in null zero random urandom; do
  touch "$root/dev/$item"
  mount --bind "/dev/$item" "$root/dev/$item"
done
mount --bind /proc "$root/proc"
mount -o remount,bind,ro,nosuid,nodev,noexec "$root/proc"
mkdir -p "$root/work" "$root/tmp" "$root/home/nix"
mount --bind "$repo" "$root/work"
# No installer, daemon, users, profile files, sudo or host store changes.
chroot "$root" /bin/bash -eu -c '
  export HOME=/home/nix USER=root
  export NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
  export NIX_CONFIG="experimental-features = nix-command flakes
build-users-group =
max-jobs = 2
cores = 2
accept-flake-config = false"
  export PATH=/nix/store/ssswpqihnyi8231b9r515k2hmpnk0h1n-nix-2.34.9/bin:/usr/bin:/bin
  nix-store --load-db < /nix/bootstrap-reginfo
  nix --version
  if [ "$1" = version ]; then exit 0; fi
  cd /work
  nix develop --no-write-lock-file .#web-build --command bash -eu -c '\''rustc --version; node --version; wasm-bindgen --version; test -d "$(rustc --print sysroot)/lib/rustlib/wasm32-unknown-unknown"; command -v wasm-opt >/dev/null; command -v zopfli >/dev/null; echo ROOTLESS_PINNED_DEVSHELL_OK'\''
' -- "$stage"
