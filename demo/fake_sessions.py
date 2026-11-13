#!/usr/bin/env python3
"""Builds a fake $HOME with synthetic Claude Code sessions for the demo video.

Usage: fake_sessions.py <fake_home>
Nothing here comes from a real machine; ages are relative to "now" so the
recording always shows fresh-looking relative times.
"""
import json, os, shutil, sys, time, uuid

HOUR, DAY = 3600, 86400

# (project, title, age_seconds, branch, prompts)
SESSIONS = [
    ("code/api", "Flaky integration test in orders service", 35 * 60, "fix/flaky-orders-test", [
        "TestCreateOrder is flaky on CI, fails about 1 in 5 runs",
        "looks like a race on the shared db fixture, can we isolate it per test?",
        "run it 200 times with -race and show me the failures",
    ]),
    ("code/storefront", "Checkout rate limiting with Upstash", 2 * HOUR, "feat/checkout-ratelimit", [
        "add rate limiting to the checkout api, 10 requests per minute per user",
        "use upstash redis, the env vars are already in .env.example",
        "write a test for the 429 path and the Retry-After header",
    ]),
    ("code/mobile", "Onboarding flow redesign", 5 * HOUR, "feat/onboarding-v2", [
        "redo the onboarding screens to match the new designs",
        "skip the permissions step if notifications are already granted",
        "add a progress indicator at the top",
    ]),
    ("notes", "Draft RFC: event sourcing for orders", 12 * HOUR, None, [
        "help me draft an RFC for moving orders to event sourcing",
        "add a section on migration and rollback",
    ]),
    ("code/storefront", "Fix hydration mismatch in cart drawer", 1 * DAY, "fix/cart-hydration", [
        "the cart drawer throws a hydration mismatch on first load",
        "it's the item count badge, it reads localStorage during render",
    ]),
    ("code/infra", "Terraform module for preview environments", 1 * DAY + 3 * HOUR, "preview-envs", [
        "write a terraform module that spins up a preview env per PR",
        "tear it down when the PR closes",
    ]),
    ("oss/tiny-router", "Release v2.0 changelog", 2 * DAY, "main", [
        "write the v2.0 changelog from the commits since v1.9",
        "call out the breaking change in route params",
    ]),
    ("code/api", "Rate limiter middleware for public API", 3 * DAY, "feat/public-ratelimit", [
        "we need a token bucket rate limiter for the /v1 public endpoints",
        "make limits configurable per api key tier",
        "return proper rate limit headers",
    ]),
    ("code/storefront", "Migrate product images to next/image", 4 * DAY, "chore/next-image", [
        "migrate all product images to next/image with proper sizes",
    ]),
    ("code/api", "Add OpenTelemetry tracing", 5 * DAY, "feat/otel", [
        "add opentelemetry tracing to the http handlers and db calls",
        "export to the collector at OTEL_EXPORTER_OTLP_ENDPOINT",
    ]),
    ("code/mobile", "Push notification deep links", 6 * DAY, "feat/deeplinks", [
        "tapping a push notification should open the right order screen",
    ]),
    ("notes", "Weekly update summary", 6 * DAY + 5 * HOUR, None, [
        "summarize what I shipped this week into a short update",
    ]),
    ("code/storefront", "Stripe webhook retries", 8 * DAY, "fix/stripe-webhooks", [
        "stripe webhooks are being processed twice on retries",
        "make the handler idempotent using the event id",
    ]),
    ("code/infra", "Cut CI time in half", 9 * DAY, "ci-speedup", [
        "our CI takes 18 minutes, find the slowest steps and parallelize them",
        "cache the go build and the node_modules",
    ]),
    ("", "Explain zsh startup slowness", 10 * DAY, None, [
        "my zsh takes 2 seconds to start, figure out why",
    ]),
    ("code/api", "Postgres connection pool tuning", 14 * DAY, "perf/pg-pool", [
        "we're seeing connection timeouts under load, tune the pgx pool",
    ]),
    ("oss/tiny-router", "Benchmark against other routers", 21 * DAY, "bench", [
        "write benchmarks comparing us with the two most popular routers",
    ]),
    ("code/mobile", "Dark mode contrast fixes", 23 * DAY, "fix/dark-contrast", [
        "fix the contrast issues in dark mode flagged by the a11y audit",
    ]),
    ("code/infra", "Rotate staging database credentials", 30 * DAY, "rotate-creds", [
        "walk me through rotating the staging db credentials without downtime",
    ]),
    ("", "Set up a new laptop", 62 * DAY, None, [
        "give me a script to set up a new mac with my usual tools",
    ]),
]


def main():
    home = os.path.abspath(sys.argv[1])
    if os.path.exists(home):
        shutil.rmtree(home)
    projects = os.path.join(home, ".claude", "projects")
    now = time.time()

    for project, title, age, branch, prompts in SESSIONS:
        cwd = os.path.join(home, project) if project else home
        os.makedirs(cwd, exist_ok=True)
        folder = os.path.join(projects, cwd.replace("/", "-").replace(".", "-"))
        os.makedirs(folder, exist_ok=True)
        sid = str(uuid.uuid4())
        lines = []
        for p in prompts:
            rec = {"type": "user", "message": {"role": "user", "content": p}, "cwd": cwd, "sessionId": sid}
            if branch:
                rec["gitBranch"] = branch
            lines.append(rec)
            lines.append({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "…"}]}})
        lines.append({"type": "ai-title", "aiTitle": title, "sessionId": sid})
        path = os.path.join(folder, sid + ".jsonl")
        with open(path, "w") as f:
            # Compact separators, like real transcripts.
            f.write("\n".join(json.dumps(l, separators=(",", ":")) for l in lines) + "\n")
        t = now - age
        os.utime(path, (t, t))

    print(f"wrote {len(SESSIONS)} fake sessions under {projects}")


if __name__ == "__main__":
    main()
