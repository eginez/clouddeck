# Troubleshooting

CloudDeck reads profiles from `~/.aws/config` and loads EC2 through the AWS
SDK, using the standard AWS sign-in cache. CloudDeck never stores credentials.
Most fixes happen in your AWS configuration or on the Connect screen. The
messages below are shown as written in the app.

Shortcuts use ⌘ on macOS and Ctrl on Linux.

## Expired SSO sign-in

Shown in the fleet failure popover:

> Your AWS sign-in has expired or is no longer valid. Sign in again, then retry.

On the Connect screen:

> The selected profile's browser session has expired. Sign in again and retry.

Sign in from CloudDeck:

1. Open `Switch Profile…` (⌘P) and choose `Connect to AWS…`.
2. Select the profile and choose `Sign in with browser`.
3. Retry the load.

If a profile that worked earlier fails with a generic `EC2 DescribeInstances
failed.` message, sign in from the Connect screen first.

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

## Profile not found or no profiles

On the welcome window:

> No AWS profiles found in ~/.aws/config.

On the Connect screen, the title is `No AWS profiles detected`, with:

> Sign in through your browser, then rescan to pick up the profile.

Check that `~/.aws/config` contains a `[profile <name>]` section. If your
profiles live somewhere else, set `Settings > AWS & Privacy > AWS config file`
to that file. Then choose `Rescan`. If the file cannot be read, CloudDeck shows
`Could not read AWS profiles:` followed by the reason.

A profile you used before may have been removed from the file. Choose another
profile on the Connect screen, or rescan after you restore it.

## Profile uses an unsupported credential provider

> This profile uses an external credential provider that CloudDeck cannot open yet. Choose another profile.

This message appears on the Connect screen. Choose a profile that uses SSO or
static access keys; CloudDeck does not open profiles that use
`credential_process`.

## Could not reach AWS

> CloudDeck could not reach AWS. Check your network connection, then retry.

On the Connect screen:

> AWS could not be reached. Check your connection and try again.

Check your network connection, proxy, or VPN, then retry.

## AWS did not respond in time

> AWS did not respond in time. Retry the load.

Retry the load. If it keeps timing out, check your network connection, proxy,
or VPN.

## Throttled requests

> AWS is limiting requests from this account. Wait a moment, then retry.

## Some regions failed

When some regions load and others fail, the fleet keeps the results it has.
With one failed region, the failure popover offers `Retry region`. With
several, each row has a `Retry` button, and `Retry all` retries every failed
region.

If a region keeps failing after a retry, check the permissions for that region
in the sections above.
