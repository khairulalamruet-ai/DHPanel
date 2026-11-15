#!/usr/bin/env bash
# create_backup for real (T09.04), on the build box, as root (sudo): the privileged helper
# runs as root, the agent code and Docker as the login user. Run it from the synced tree
# (`deploy/dev/remote.sh sh "sudo -n bash deploy/dev/backup-live.sh"`).
