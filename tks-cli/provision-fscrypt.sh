#!/bin/sh
# Run this script to provision fscrypt on your system
# fscrypt is being enabled for all users on the system so we could later use
# it to encrypt tks-service's data directory

# check if fscrypt is installed
if ! command -v fscrypt &> /dev/null
then
    echo "fscrypt could not be found. Please install fscrypt and try again."
    exit
fi

# NOTE: this should be the same as the one specified in tks-service's config
storage_path="${HOME}/.local/share/io.linux-tks/storage"
home_fs=$(df -h /home | awk 'NR==2{print $1}')

fscrypt_status=$(fscrypt status)
encryption_not_supported=$( echo "$fscrypt_status" | grep "filesystems supporting.*0")
if ! [ -z "$encryption_not_supported" ]
then
    echo "No filesystems supporting fscrypt found."
    echo "You may want to run the following command then try again:"
    echo "sudo tune2fs -O encrypt $home_fs"
    exit
fi

# check if fscrypt is enabled globally
fscrypt_global_status=$(fscrypt status | grep "filesystems with fscrypt.*: [^0]")
if ! [ -z "$fscrypt_global_status" ]
then
    echo "fscrypt is already enabled on this system."
else
  echo "fscrypt is not enabled on this system. Enabling fscrypt..."
  sudo fscrypt setup
fi

# enable fscrypt on user's home partition filesystem
home_mount_point=$(df -h /home | awk 'NR==2{print $6}')
fscrypt_enabled=$(fscrypt status | grep "$home_mount_point.*supported")
if [ -z "$fscrypt_enabled" ]
then
  echo "fscrypt is not enabled on $home_mount_point. Enabling fscrypt on $home_mount_point..."
  sudo fscrypt setup $home_mount_point --all-users
  echo "fscrypt has been enabled on $home_mount_point."
fi

for f in $home_mount_point/.fscrypt/protectors/*; do
  protector=$(basename $f)
  protector_found=$(fscrypt metadata dump --protector $home_mount_point:$protector | grep "tks-service")
  if ! [ -z "$protector_found" ]
  then
    protector_id=$protector
    echo "tks-service protector found: $protector_id"
    break
  fi
done

if [ -z "$protector_id" ]
then
  echo "tks-service protector not found. Creating tks-service protector..."
  fscrypt metadata create protector $home_mount_point \
    --name tks-service \
    --source="custom_passphrase"
  for f in $home_mount_point/.fscrypt/protectors/*; do
    protector=$(basename $f)
    protector_found=$(fscrypt metadata dump $home_mount_point:$f | grep "tks-service")
    if ! [ -z "$protector_found" ]
    then
      protector_id=$f
      echo "tks-service protector created: $protector_id"
      break
    fi
  done
fi

# OK, fscrypt is enabled. Now we can prepare tks data directory
echo "Preparing tks data directory..."
if tks-cli service status &> /dev/null
then
    echo "tks service is running. Stopping tks service..."
    tks-cli service stop
fi

