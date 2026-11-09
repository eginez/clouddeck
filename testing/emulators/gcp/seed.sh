#!/bin/sh
# Runs inside the seed container. Creates sample projects, networks and instances in the floci-gcp emulator.
set -eu

endpoint="${FLOCI_GCP_ENDPOINT:-http://floci-gcp:4588}"

post() { # path json
    code=$(curl -s -o /tmp/response -w '%{http_code}' -X POST \
        -H 'Content-Type: application/json' -d "$2" "$endpoint$1")
    case "$code" in
        2*|409) ;;
        *)
            echo "POST $1 failed with HTTP $code:" >&2
            cat /tmp/response >&2
            echo >&2
            exit 1
            ;;
    esac
}

instance() { # project zone region name machine-type env
    post "/compute/v1/projects/$1/zones/$2/instances" "{
        \"name\": \"$4\",
        \"machineType\": \"zones/$2/machineTypes/$5\",
        \"labels\": {\"env\": \"$6\", \"team\": \"core\"},
        \"disks\": [{
            \"boot\": true,
            \"autoDelete\": true,
            \"initializeParams\": {
                \"sourceImage\": \"projects/debian-cloud/global/images/family/debian-12\",
                \"diskSizeGb\": \"10\"
            }
        }],
        \"networkInterfaces\": [{
            \"subnetwork\": \"regions/$3/subnetworks/default\",
            \"accessConfigs\": [{\"name\": \"External NAT\", \"type\": \"ONE_TO_ONE_NAT\"}]
        }]
    }"
}

for project in demo-web demo-data; do
    post /v1/projects "{\"projectId\": \"$project\", \"name\": \"$project\"}"
    post "/compute/v1/projects/$project/global/networks" \
        '{"name": "default", "autoCreateSubnetworks": false}'
    post "/compute/v1/projects/$project/regions/us-central1/subnetworks" \
        '{"name": "default", "network": "global/networks/default", "ipCidrRange": "10.128.0.0/20", "region": "regions/us-central1"}'
    post "/compute/v1/projects/$project/regions/europe-west1/subnetworks" \
        '{"name": "default", "network": "global/networks/default", "ipCidrRange": "10.132.0.0/20", "region": "regions/europe-west1"}'
done

instance demo-web us-central1-a us-central1 web-1 c3-standard-4 prod
instance demo-web us-central1-b us-central1 web-2 e2-standard-2 staging
instance demo-web us-central1-a us-central1 api-1 n2-standard-4 prod
instance demo-web europe-west1-b europe-west1 edge-1 e2-standard-2 prod
instance demo-data us-central1-a us-central1 warehouse-1 n2-standard-8 prod
instance demo-data us-central1-b us-central1 etl-1 n2-standard-4 staging
instance demo-data europe-west1-b europe-west1 notebook-1 g2-standard-4 dev
instance demo-data us-central1-b us-central1 gpu-train-1 g2-standard-8 dev

post "/compute/v1/projects/demo-web/zones/us-central1-b/instances/web-2/stop" '{}'

echo "seeded GCP emulator: projects demo-web and demo-data, 8 instances"
