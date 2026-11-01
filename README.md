# CloudDeck

**See all your EC2 instances in one place.**

CloudDeck is a desktop app for macOS and Linux that lists the EC2 instances of
your AWS accounts and regions in one searchable table. Pick a profile, pick the
regions, type a few words, and find the machine you are looking for without
clicking through the AWS console.

It is read only: it never changes anything in your cloud. It keeps no
credentials of its own. It uses the AWS profiles you already have in
`~/.aws/config` and the standard AWS sign-in cache (the same one the AWS CLI
uses for SSO), so your sign-in and permissions work as they always have.

![CloudDeck main window, light theme](design/images/01-main-window-light.png)

![CloudDeck main window, dark theme](design/images/02-main-window-dark.png)

## Install

**macOS (Apple silicon), with Homebrew:**

```bash
brew install --cask eginez/clouddeck/clouddeck
```

**Linux (x86_64), with Homebrew:**

```bash
brew install eginez/clouddeck/clouddeck
```

**Or download it** from the [Releases page](https://github.com/eginez/clouddeck/releases):
a disk image for macOS, or a tarball for Linux (extract it and run `./clouddeck`).

The macOS app is not notarized by Apple yet. If you download it by hand, the
first time you open it, right-click CloudDeck and choose Open. The Homebrew
cask does this step for you.

CloudDeck checks for new versions and offers to install them. Nothing installs
without your say so.

## Use it

1. Have an AWS profile in `~/.aws/config` (made with `aws configure` or
   `aws configure sso`, or written by hand). For an IAM Identity Center (SSO)
   profile, CloudDeck can sign you in itself, in your browser.
2. Start CloudDeck. It lists your profiles. Choose one and press Enter.
3. Choose the regions to load. A region that fails does not hide the others,
   and you can retry it.
4. Search. Type words, or filter by field:

```text
web                  instances whose metadata matches "web"
state:running        only running instances
type:t3.*            any t3 instance size
-state:stopped       everything except stopped instances
tag:Team=payments    a tag value
```

Click an instance to see its details in the inspector. Press ⌘P (Ctrl+P on
Linux) to switch profile.

Want to look around first? Run `clouddeck --demo` to use fake data with no AWS
account.

## What you need

- An AWS profile in `~/.aws/config` (the AWS CLI is not required).
- Read only permission for `ec2:DescribeInstances` in the regions you look at.
- On Linux: a Vulkan driver, libxkbcommon, fontconfig, and a Wayland or X11
  session.

If something does not load, see [docs/troubleshooting.md](docs/troubleshooting.md).

## Contributing

Building from source, running the tests and sending changes are described in
[CONTRIBUTING.md](CONTRIBUTING.md). Report security issues as described in
[SECURITY.md](SECURITY.md). Everyone taking part follows the
[Code of Conduct](CODE_OF_CONDUCT.md).

## License

CloudDeck is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in CloudDeck by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
