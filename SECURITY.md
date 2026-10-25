# Security Policy

## Supported Versions

Security fixes are made for the latest release of CloudDeck. Older releases
are not patched.

## Reporting a Vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub's private vulnerability reporting on this
repository: open the **Security** tab and choose **Report a vulnerability**.

Include, where you can:

- the CloudDeck version or commit you tested,
- the operating system and how CloudDeck was started,
- steps to reproduce, and the impact you expect.

You can expect an acknowledgement within a few business days. We will keep you
informed while the issue is investigated and fixed, and will credit you in the
release notes unless you ask us not to.

CloudDeck only reads EC2 metadata through the AWS CLI and does not store
credentials, so reports about credential handling or unexpected AWS requests
are especially welcome.
