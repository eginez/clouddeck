#!/bin/sh
# Runs inside the seed container. Creates sample EC2 instances in the floci emulator.
set -eu

export AWS_PAGER=""
endpoint="${FLOCI_ENDPOINT:-http://floci:4566}"

launch() { # region name type env -> prints the instance id
    aws --endpoint-url "$endpoint" --region "$1" ec2 run-instances \
        --image-id ami-12345678 --instance-type "$3" --count 1 \
        --tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=$2},{Key=env,Value=$4}]" \
        --query 'Instances[0].InstanceId' --output text
}

stop() { # region instance-id
    aws --endpoint-url "$endpoint" --region "$1" ec2 stop-instances --instance-ids "$2" >/dev/null
}

terminate() { # region instance-id
    aws --endpoint-url "$endpoint" --region "$1" ec2 terminate-instances --instance-ids "$2" >/dev/null
}

us="us-east-1"
eu="eu-west-1"

launch "$us" web-prod-1 t3.small prod >/dev/null
launch "$us" web-prod-2 t3.small prod >/dev/null
launch "$us" api-prod-1 t3.medium prod >/dev/null
api_staging=$(launch "$us" api-staging-1 t3.medium staging)
launch "$us" worker-staging-1 m6i.large staging >/dev/null
launch "$us" batch-dev-1 m6i.large dev >/dev/null
db_dev=$(launch "$us" db-dev-1 t3.medium dev)
web_dev=$(launch "$us" web-dev-1 t3.small dev)

launch "$eu" eu-web-1 t3.small prod >/dev/null
launch "$eu" eu-api-1 m6i.large prod >/dev/null
eu_worker=$(launch "$eu" eu-worker-1 t3.medium staging)
launch "$eu" eu-batch-1 m6i.large dev >/dev/null

stop "$us" "$api_staging"
stop "$us" "$db_dev"
stop "$eu" "$eu_worker"
terminate "$us" "$web_dev"

echo "seeded AWS emulator: 12 instances in $us and $eu"
