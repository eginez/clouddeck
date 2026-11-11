# Local cloud emulators

Run CloudDeck against fake AWS and GCP instead of real accounts. The emulators
are [floci](https://github.com/floci-io/floci) (AWS, port 4566) and
[floci-gcp](https://github.com/floci-io/floci-gcp) (GCP, port 4588). Nothing
here touches your real `~/.aws` or `~/.config/clouddeck`, and the credentials
are fake.

## 1. Install Docker

Install one of:

- [Docker Desktop](https://www.docker.com/products/docker-desktop/), or


Start it and wait until it reports that it is running.

## 2. Start the emulators

From the repository root:

```sh
./testing/emulators/run.sh start all
```

This starts both emulators, waits until they are healthy, and loads the sample
data. The first run downloads the images and takes a few minutes. Later runs
are quick.

`start` prints the steps for step 3.

## 3. Run CloudDeck

AWS, in CloudDeck:

1. Choose Connect (on first run, or from the provider picker, Connect again).
2. Choose Amazon Web Services, then the method Use an AWS profile.
3. Set Config folder to `/path/to/repo/testing/emulators/aws` (Browse, or type it).
4. Pick `local-dev` and choose Connect.

You should see the profiles `local-dev` (us-east-1) and `local-prod` (eu-west-1),
each with EC2 instances in Running, Stopped and Terminated states, and
`local-down` failing to load, on purpose: nothing listens on port 4599.

GCP, in CloudDeck:

1. Choose Connect, then Google Cloud.
2. Choose the method Use gcloud application-default credentials.
3. Set Config folder to `/path/to/repo/testing/emulators/gcp`.
4. Choose Connect.

You should see the projects `demo-web` and `demo-data` with a region and zone
tree of instances. The All providers view shows both AWS and GCP.

Without the app setting, AWS can also read the emulator profiles from the
environment. In the shell you start CloudDeck from, run:

```sh
export AWS_CONFIG_FILE=/path/to/repo/testing/emulators/aws/config AWS_SHARED_CREDENTIALS_FILE=/path/to/repo/testing/emulators/aws/credentials
clouddeck
```

Or build and run from source, after the `export` line:

```sh
scripts/stage-bundled-extensions.sh debug
cargo run -p clouddeck-app
```

Reset the app to your real account: open Settings, clear the Config folder
fields for Amazon Web Services and Google Cloud, then sign in to your own
accounts as usual.

## 4. Stop

```sh
./testing/emulators/run.sh stop all
```

`stop` keeps the containers and their data, so the next `start` picks up where
you left off. To start over with fresh sample data:

```sh
./testing/emulators/run.sh reset all
```

To see what is running:

```sh
./testing/emulators/run.sh status all
```

Each cloud can be started on its own, with `aws` or `gcp` in place of `all`.

## Files

- `run.sh`: starts, stops, resets and reports on the emulators
- `aws/docker-compose.yml`, `aws/seed.sh`: floci and the AWS sample EC2 instances
- `aws/config`, `aws/credentials`: sample AWS profiles with fake keys
- `gcp/docker-compose.yml`, `gcp/seed.sh`: floci-gcp and the GCP sample projects
- `gcp/emulator.json`, `gcp/application_default_credentials.json`: sample GCP files with fake values

`run.sh start` writes a `.seeded` marker in each cloud folder so the sample data
loads once. `reset` removes it. If you remove the containers by hand (for example
with `docker compose down`), run `reset` too, or the marker will say the data is
loaded when the emulator is empty.

Check the launcher without Docker:

```sh
scripts/test-emulators-dry-run.sh
```
