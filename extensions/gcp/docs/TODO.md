# Not done yet (file as beads when the extension leaves local-only)

- Browser sign-in (OAuth installed-app flow with PKCE, `AuthStep::OpenUrl`): needs a registered OAuth client id for CloudDeck.
- Service-account credentials (JWT bearer grant): `parse_adc` currently refuses `type: service_account` with an Auth error.
- Metadata-server credentials (GCE/Cloud Run) and `gcloud auth print-access-token` fallback.
- Other kinds beyond instances (disks, buckets, GKE clusters, Cloud SQL).
- Inspector sections, richer `fields()` (labels as facets, machine family).
- Live smoke test against a real project; the fixtures are hand-written to match the documented API shapes.
- Narrow `aggregatedList` server-side with `filter=zone eq ...` when a selection is small.
