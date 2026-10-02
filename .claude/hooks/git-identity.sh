#!/bin/sh
# Author commits as the human developer, never as the agent.
# Each dev sets GLANCE_DEV_NAME and GLANCE_DEV_EMAIL in their own environment.
[ -n "$GLANCE_DEV_NAME" ] && [ -n "$GLANCE_DEV_EMAIL" ] || exit 0
git config --local user.name "$GLANCE_DEV_NAME"
git config --local user.email "$GLANCE_DEV_EMAIL"
