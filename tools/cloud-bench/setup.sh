#!/usr/bin/env bash
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# One-time set-up of a Google Cloud project for the cloud render bench (D-042).
#
#     tools/cloud-bench/setup.sh PROJECT GITHUB_REPOSITORY
#
# Run it as a person who owns the project, with billing already linked. It
# creates, and can be run again over what it already created:
#
# - a network with no inbound rule: bench machines only reach out;
# - a bucket where a run's build goes in and its receipts come out, emptied
#   after 30 days;
# - two service accounts: `bench-vm`, which the bench machine runs as and
#   which can only touch that bucket, and `bench-ci`, which GitHub Actions
#   acts as to create and delete bench machines;
# - a workload identity pool that lets GitHub Actions act as `bench-ci`
#   without any stored key, only for that repository's `main` branch.
#
# It prints the repository variables the workflow reads.

set -euo pipefail

project=${1:?project}
repository=${2:?owner/name of the GitHub repository}
region=asia-southeast1
bucket="$project-runs"
number=$(gcloud projects describe "$project" --format='value(projectNumber)')
repository_id=$(gh api "repos/$repository" --jq .id)
g() { gcloud --project "$project" --quiet "$@"; }
exists() { "$@" >/dev/null 2>&1; }

g services enable compute.googleapis.com storage.googleapis.com iam.googleapis.com \
  iamcredentials.googleapis.com sts.googleapis.com

# The network. Without a rule, nothing comes in; going out is allowed.
exists g compute networks describe bench ||
  g compute networks create bench --subnet-mode custom
exists g compute networks subnets describe bench --region "$region" ||
  g compute networks subnets create bench --network bench --region "$region" --range 10.20.0.0/24

# The bucket.
exists gcloud storage buckets describe "gs://$bucket" ||
  gcloud storage buckets create "gs://$bucket" --project "$project" --location "$region" \
    --uniform-bucket-level-access --public-access-prevention
lifecycle=$(mktemp)
echo '{"rule": [{"action": {"type": "Delete"}, "condition": {"age": 30}}]}' >"$lifecycle"
gcloud storage buckets update "gs://$bucket" --lifecycle-file "$lifecycle"
rm "$lifecycle"

# The service accounts.
for account in bench-vm bench-ci; do
  exists g iam service-accounts describe "$account@$project.iam.gserviceaccount.com" ||
    g iam service-accounts create "$account" --display-name "osage $account"
done
vm="bench-vm@$project.iam.gserviceaccount.com"
ci="bench-ci@$project.iam.gserviceaccount.com"
for account in "$vm" "$ci"; do
  gcloud storage buckets add-iam-policy-binding "gs://$bucket" \
    --member "serviceAccount:$account" --role roles/storage.objectAdmin >/dev/null
done
g projects add-iam-policy-binding "$project" --member "serviceAccount:$ci" \
  --role roles/compute.instanceAdmin.v1 --condition None >/dev/null
g iam service-accounts add-iam-policy-binding "$vm" --member "serviceAccount:$ci" \
  --role roles/iam.serviceAccountUser >/dev/null

# GitHub Actions, without a stored key: tokens from that repository's main
# branch, pushed, scheduled or started by hand, and nothing else (D-041:
# merged code only; pull requests never get a token).
exists g iam workload-identity-pools describe github --location global ||
  g iam workload-identity-pools create github --location global --display-name "GitHub Actions"
condition="assertion.repository_id == '$repository_id' && assertion.ref == 'refs/heads/main' && assertion.event_name in ['push', 'schedule', 'workflow_dispatch']"
mapping="google.subject=assertion.sub,attribute.repository_id=assertion.repository_id,attribute.ref=assertion.ref"
if exists g iam workload-identity-pools providers describe osage --location global --workload-identity-pool github; then
  g iam workload-identity-pools providers update-oidc osage --location global --workload-identity-pool github \
    --attribute-mapping "$mapping" --attribute-condition "$condition"
else
  g iam workload-identity-pools providers create-oidc osage --location global --workload-identity-pool github \
    --display-name "$repository" --issuer-uri https://token.actions.githubusercontent.com \
    --attribute-mapping "$mapping" --attribute-condition "$condition"
fi
pool="projects/$number/locations/global/workloadIdentityPools/github"
g iam service-accounts add-iam-policy-binding "$ci" --role roles/iam.workloadIdentityUser \
  --member "principalSet://iam.googleapis.com/$pool/attribute.repository_id/$repository_id" >/dev/null

cat <<EOF

Set up. Repository variables for .github/workflows/cloud-bench.yml:

  OSAGE_BENCH_PROJECT=$project
  OSAGE_BENCH_PROVIDER=$pool/providers/osage
  OSAGE_BENCH_SERVICE_ACCOUNT=$ci

  gh variable set OSAGE_BENCH_PROJECT --repo $repository --body $project
  gh variable set OSAGE_BENCH_PROVIDER --repo $repository --body $pool/providers/osage
  gh variable set OSAGE_BENCH_SERVICE_ACCOUNT --repo $repository --body $ci

Then build the machine image: tools/cloud-bench/make-image.sh $project
EOF
