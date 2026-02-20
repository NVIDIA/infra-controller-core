#!/bin/bash
set -eu -o pipefail

# egx.conf file on the host system
ticker=900
egx_conf_db="/host/etc/egx.conf"
EGX_ADMIN_PASSWORD=${EGX_ADMIN_PASSWORD:-""}
EGX_RCPROXY_PASSWORD=${EGX_RCPROXY_PASSWORD:-"YesIAuthorize#1234"}

# Set expiration for all active users to infinite
ensure_infinite_passwd_expiration() {
  users=$(awk -F: '$3 >= 1000 && $1 != "nobody" {print $1}' /host/etc/passwd)
  echo "[BEG] Set password expiration for all users to infinite"
  for user in $users; do
    nsenter -t 1 -m -u -n -i chage -I -1 -m 0 -M 99999 -E -1 "$user" || true
  done
  echo "[END] Set password expiration for all users to infinite"
}

# State management functions
egx_kv_get() {
  local -r f="$1" key="$2"
  sed -n "s/^${key}=\"\([^\"]*\)\"$/\1/p" "${f}"
}

egx_kv_must_get() {
  local val
  val="$(egx_kv_get "$1" "$2")"
  if [[ -z "${val}" ]]; then
    fatal "Can not get value for key '$2' from '$1'"
  fi
  echo "${val}"
}

# reset_admin_passwd pulls the admin password from egx.conf and sets it again
# this handles the case where a user expires the password, but without
# a way to change it
reset_admin_passwd() {

  if [[ -z "${EGX_ADMIN_PASSWORD}" ]]; then
    local admin_encrypted_pass
    echo "[BEG] Resetting admin password from ${egx_conf_db}"
    admin_encrypted_pass="$(egx_kv_must_get "${egx_conf_db}" EGX_ADMIN_ENCRYPTED_PASS)"
    echo "Setting the encrypted password value ${admin_encrypted_pass} for user admin"
    nsenter -t 1 -m -u -n -i sh -c "echo \"admin:${admin_encrypted_pass}\" | chpasswd -e" || true
    echo "[END] Resetting admin password from ${egx_conf_db}"
  else
    echo "Setting the password for user admin"
    nsenter -t 1 -m -u -n -i sh -c "echo \"admin:\$(mkpasswd -m sha-512 "${EGX_ADMIN_PASSWORD}")\" | chpasswd -e" || true
    echo "[END] Resetting admin password from ${egx_conf_db}"
  fi

  echo "[BEG] Resetting rcproxy password"
  nsenter -t 1 -m -u -n -i sh -c "echo rcproxy:\$(mkpasswd -m sha-512 "${EGX_RCPROXY_PASSWORD}") | chpasswd -e" || true
  echo "[END] Resetting rcproxy password"
}

main() {
  echo "Every ${ticker} seconds reset the admin password and set all accounts to infinite expiration..."
  while true; do
    echo "[BEG] unexpiring account passwords"
    ensure_infinite_passwd_expiration
    reset_admin_passwd
    echo "[END] unexpiring account passwords"
    echo "Waiting 15 minutes before re-running"
    sleep 900
  done 
}

main "$@"