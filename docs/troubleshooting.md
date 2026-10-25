# Troubleshooting

CloudDeck never stores credentials. Sign-in, SSO tokens, and permissions are
handled by the AWS CLI, so most fixes happen in your AWS configuration or in
the Connect screen. The messages below are shown as written in the app.

Shortcuts use ⌘ on macOS and Ctrl on Linux.

## Expired SSO sign-in

Shown in the fleet failure popover:

> Your AWS sign-in has expired or is no longer valid. Sign in again, then retry.

On the Connect screen:

> The selected profile's browser session has expired. Sign in again and retry.

Fix it in one of two ways:

- Open `Switch Profile…` (⌘P), choose `Connect to AWS…`, select the profile,
  and choose `Sign in with browser`.
- Sign in with the AWS CLI for that profile:
  `aws sso login --profile <name>`. CloudDeck reads the refreshed session the
  next time it loads.

CloudDeck also treats AWS CLI output that mentions an expired SSO session as a
sign-in problem, so the same advice appears when the CLI reports one.

## AccessDenied or permission errors

Shown in the fleet failure popover:

> The selected profile or role is not allowed to make this request. Check the IAM permissions for that profile or role, then retry.

On the Connect screen, the title is `AWS access is restricted`, with:

> AWS confirmed your identity, but this profile cannot describe EC2 in the selected region.

The profile or role needs read access to EC2 in the region you selected:

> EC2 DescribeInstances was denied. Verify the selected profile has ec2:DescribeInstances permission.

Identity checks need `sts:GetCallerIdentity`. If that call is denied, CloudDeck
reports:

> AWS identity access was denied. Verify sts:GetCallerIdentity permission.

## Missing region

On the Connect screen, the title is `Choose a region to continue`, with:

> Select an AWS region for the EC2 fleet, then connect.

Choose a region on the Connect screen before connecting. A profile's `region`
in `~/.aws/config` is only used as the initial selection.

## No profiles found

On the welcome window:

> No AWS profiles found in ~/.aws/config.

On the Connect screen, the title is `No AWS profiles detected`, with:

> Sign in through your browser, then rescan to pick up the profile.

Check that `~/.aws/config` contains a `[profile <name>]` section, or that
`AWS_CONFIG_FILE` points to the file you expect. Then choose `Rescan`. If the
file cannot be read, CloudDeck shows `Could not read AWS profiles:` followed by
the reason.

## Profile uses an unsupported credential provider

> This profile uses an external credential provider that CloudDeck cannot open yet. Choose another profile.

This message appears on the Connect screen. Choose a profile that uses SSO or
static access keys; CloudDeck does not open profiles that use
`credential_process`.

## AWS CLI not found or too old

> AWS CLI not found at <path>. Set the path in Settings > AWS & Privacy or install AWS CLI v2.

> AWS CLI v1 is not supported at <path> (<version>). Install AWS CLI v2: https://aws.amazon.com/cli/

Install AWS CLI v2, or set the path to it under `Settings > AWS & Privacy`.
A path set in Settings is used first. Otherwise CloudDeck uses the first of
`/opt/homebrew/bin/aws`, `/usr/local/bin/aws`, and `/usr/bin/aws` that exists,
and falls back to `aws` on `PATH`.

## Could not reach AWS

> CloudDeck could not reach AWS. Check your network connection, then retry.

On the Connect screen:

> AWS could not be reached. Check your connection and try again.

Check your network connection, proxy, or VPN, then retry.

## AWS did not respond in time

> AWS did not respond in time. Retry the load.

## Throttled requests

> AWS is limiting requests from this account. Wait a moment, then retry.

## Some regions failed

When some regions load and others fail, the fleet keeps the results it has.
With one failed region, the failure popover offers `Retry region`. With
several, each row has a `Retry` button, and `Retry all` retries every failed
region.
