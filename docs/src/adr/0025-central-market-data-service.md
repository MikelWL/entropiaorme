# ADR-0025: A central market-data service on AWS serverless

- Status: Accepted
- Context: the market layer (ADR-0024) is fed by each installation's own pastes. Sharing those observations between installations needs the one thing the app architecture cannot provide locally: a central ingest, aggregation, and distribution point. This is the codebase's first and only backend service.

## Context and problem statement

A single installation's markup observations go stale at the pace of one player's paste habit. Pooling observations across installations keeps every client fresher than its own feed, but requires a service that can receive authenticated submissions, aggregate them, and hand every client the same versioned result. The workload shape is extreme: submissions are a weekly-cadence trickle, reads are cacheable JSON fan-out, and days can pass with no traffic at all. An always-on server would idle at full price; the service also must not weaken the app's posture (no telemetry, opt-in networking, the ADR-0024 wall).

## Decision

A standalone service, [market-data-service](https://github.com/entropiaorme/market-data-service), on AWS serverless with every resource in Terraform:

- **Pipeline.** An API Gateway HTTP API fronts a Rust ingest Lambda (bearer-token auth, JSON Schema validation, idempotent writes) into a single DynamoDB table; a scheduled aggregation Lambda folds observations into a versioned snapshot JSON published to S3 behind CloudFront, with an atomic `latest` pointer and a five-minute cache window. Submission and snapshot contracts are versioned JSON Schemas published at the URLs their `$id` fields declare.
- **Trust model.** Contributors hold individually minted bearer tokens; every observation carries provenance. Aggregation is written for N contributors and currently exercised by one; outlier handling is the named gate before enrolment ever widens.
- **Client posture.** All service traffic routes through the app's one hardened outbound gateway (`app/src/lib/outboundHttp.ts`), pinned in the CSP alongside the news feed. Consuming the shared snapshot is a consent choice offered at first run; contributing is a second, independent, default-off opt-in that additionally requires a token, and each send is an explicit user action (`app/src/lib/marketDataFetch.ts`). Snapshot markup lands in the same quarantined informational layer as local pastes.
- **Cross-origin posture.** Both service surfaces answer webview cross-origin requests with wildcard-origin CORS (the HTTP API's CORS configuration for the authenticated submission preflight; bucket CORS plus forwarded request headers for the snapshot's conditional fetch). The wildcard is deliberate: authentication is the bearer token, requests carry no ambient credentials, and the webview origin differs by platform.
- **Platform.** Scale-to-zero serverless fits the traffic shape at effectively free-tier cost; gateway throttling caps abuse ahead of the bill, budget and error alarms are part of the stack, and Terraform owns every resource so the whole service is reproducible from the repository.

## Consequences

Every installation can read fresher markup than it feeds itself, and the app's privacy claims survive: consumption is a consent-gated download of public game-economy data, contribution is a deliberate act, and nothing session-identifying leaves the client. The service runs at hobby scale and says so; the cost of the serverless shape is cold starts measured in milliseconds against a weekly cadence, which is no cost at all.

Two operational lessons are part of this record. First, S3 answers a missing key with an access-denied error unless the reader also holds list permission, so the first aggregation run could not distinguish "nothing published yet" from a policy fault; the fix is a prefix-scoped list grant, and the error alarm proving itself on that incident validated the alert path. Second, the original verification exercised the service with command-line tools and never through the app's webview, which left the entire CORS layer missing until real use: the submission preflight failed closed with no server-side trace at all. A webview client's loop is only verified from inside a webview.

See [ADR-0024](0024-market-informational-layer.md) for the informational-layer wall the service feeds into, [ADR-0011](0011-etag-conditional-requests.md) for the conditional-request pattern the snapshot fetch reuses client-side, and the [ADR index](index.md).
