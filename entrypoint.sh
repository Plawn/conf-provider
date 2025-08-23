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

exec /app/server git --repo-url "$REPO_URL" --branch "$BRANCH" "$@"