#!/bin/sh
set -e

if [ -z "$REPO_URL" ]; then
  echo "ERROR: REPO_URL must be set"
  exit 1
fi

if [ -z "$BRANCH" ]; then
  echo "ERROR: BRANCH must be set"
  exit 1
fi

if [ -z "$USERNAME" ]; then
  echo "ERROR: USERNAME must be set"
  exit 1
fi

if [ -z "$PASSWORD" ]; then
  echo "ERROR: PASSWORD must be set"
  exit 1
fi

exec /app/server git --repo-url "$REPO_URL" --branch "$BRANCH" --username "$USERNAME" --password "$PASSWORD" "$@"