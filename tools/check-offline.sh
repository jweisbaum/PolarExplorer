#!/usr/bin/env bash
# Enforces invariant 4: nothing is fetched that the user did not ask for, and
# HTTP is limited to two crates; pe-app holds the MCP service's loopback
# listener (D29), whose server crates layer 2 admits and nothing else. pe-app
# alone may read the person's SYRF PostgreSQL database, read-only and only when
# asked (asked 2026-10-06); no other crate has a database client.
#
# Adapted from VectorEffects. Four layers, each checking what it can check:
#
#   1. Hand-written sources     -- any absolute URL is suspicious, so flag all,
#      except in `pe-env` and `pe-trackers`, where every URL must name one of
#      that crate's allow-listed hosts.
#   2. Dependencies             -- a network client crate may be a direct
#      dependency of those two crates only, and no C library that the five
#      targets cannot all build (OpenSSL, blosc) may be in the lockfile.
#   3. The built bundle         -- only remote *resource references* are flagged.
#      A minified bundle is full of documentation URLs inside error strings
#      (React links to react.dev when it throws); scanning it for `http` finds
#      hundreds of those and nothing useful. What matters is whether the HTML or
#      CSS actually loads something remote.
#   4. The CSP                  -- the runtime enforcement. Even if a remote URL
#      slipped through, `default-src 'self'` stops the load. So the CSP itself
#      is checked for having been weakened.
set -uo pipefail
cd "$(dirname "$0")/.."

CONF=crates/pe-app/tauri.conf.json

# Legitimately present, never fetched by the shipped app: the JSON schema
# reference, the dev server, and the Tauri IPC origin.
# `.localhost` is a reserved TLD (RFC 6761) that never resolves off-machine, so
# the Tauri IPC origin's Windows form is local by definition.
# The SVG XML namespace is an identifier, never a location: an XML parser
# compares it as a string and dereferences nothing.
# `.invalid` is reserved by RFC 2606 and never resolves, which is what makes it
# the right host for a test that must not reach anything.
ALLOW='schema\.tauri\.app|//localhost:|\.localhost|//127\.0\.0\.1|www\.w3\.org/2000/svg|\.invalid'
# Pure comment lines. A URL in a comment fetches nothing, and the generated
# ts-rs bindings carry a provenance URL in their header.
COMMENT=':[0-9]+:[[:space:]]*(//|\*|/\*)'
# A URL as it appears in source: up to a quote, space, bracket or backtick.
URL='https?://[^"'"'"'`[:space:])>]+'

# The two network crates and their hosts (CLAUDE.md invariant 4). Each URL in
# them must *start* with one of these, so a line naming an allowed host cannot
# smuggle a second one past the check. `{…}` stands for a format placeholder
# in a subdomain, which is how Geovoile's per-race hosts are built.
# pe-env names no archive: it reads Hindsight Sync on loopback and the
# location Hindsight Sync hands it, so its hosts are 127.0.0.1 and Amazon
# S3's virtual-hosted template. The last alternative is not a host it reads
# but an expected value: the address `net.rs`'s and `whirlwind/tests.rs`'s
# tests check the template expands to.
ENV_HOSTS='^https?://(127\.0\.0\.1([/:?#]|$)|\{bucket\}\.s3\.\{region\}\.amazonaws\.com([/:?#]|$)|whirlwind-hindsight\.s3\.us-east-1\.amazonaws\.com/hindsight-routing)'
TRACKER_HOSTS='^https?://(((cf|app)\.)?yb\.tl|([A-Za-z0-9_{}-]+\.)*geovoile\.com|(api|race)\.bluewatertracks\.com|www\.regattaman\.com|data\.orc\.org)([/:?#]|$)'

fail=0
report() {
  echo "OFFLINE CHECK FAILED -- $1:"
  echo "$2" | sed 's/^/  /'
  fail=1
}

# Every URL on the non-comment lines of the given .rs files under a crate,
# as `file:line:url`, minus the always-local ones.
urls_in() {
  find "$1" -name '*.rs' -exec grep -nHE 'https?://' {} + 2>/dev/null \
    | grep -vE "$COMMENT" \
    | while IFS= read -r line; do
        loc=$(echo "$line" | cut -d: -f1-2)
        echo "$line" | cut -d: -f3- | grep -oE "$URL" | grep -vE "$ALLOW" | sed "s|^|$loc:|"
      done
}

# --- 1. Hand-written sources -------------------------------------------------
hits=$(grep -rnE 'https?://' ui/src ui/index.html 2>/dev/null \
  | grep -vE "$COMMENT" | grep -vE "$ALLOW" || true)
[ -n "$hits" ] && report "absolute URL in frontend source" "$hits"

# Everywhere but the two network crates, a URL in Rust is a finding, so a fetch
# that creeps into another crate is caught -- which is the whole point of
# naming the exceptions here.
hits=$(find crates -name '*.rs' -not -path 'crates/pe-env/*' -not -path 'crates/pe-trackers/*' \
  -not -path 'crates/pe-app/src/library/tests.rs' -not -path 'crates/pe-app/src/library/scrape/tests.rs' \
  -exec grep -nHE 'https?://' {} + 2>/dev/null \
  | grep -vE "$COMMENT" | grep -vE "$ALLOW" || true)
[ -n "$hits" ] && report "absolute URL in rust source" "$hits"

# In those crates, only the hosts they exist to read.
hits=$(urls_in crates/pe-env | grep -vE ":[0-9]+:${ENV_HOSTS#^}" || true)
[ -n "$hits" ] && report "unexpected remote host in pe-env" "$hits"

hits=$(urls_in crates/pe-trackers | grep -vE ":[0-9]+:${TRACKER_HOSTS#^}" || true)
[ -n "$hits" ] && report "unexpected remote host in pe-trackers" "$hits"

# The cfg(test)-only database integration tests exercise the real tracker
# clients against an isolated DB. Apply the same host restrictions to their URLs.
hits=$(urls_in crates/pe-app/src/database/tests.rs | grep -vE ":[0-9]+:${TRACKER_HOSTS#^}" || true)
[ -n "$hits" ] && report "unexpected remote host in database tests" "$hits"

hits=$(grep -nE 'https?://' "$CONF" 2>/dev/null | grep -vE "$ALLOW" || true)
[ -n "$hits" ] && report "absolute URL in tauri.conf.json" "$hits"

# --- 2. Dependencies ---------------------------------------------------------
# A network client named as a direct dependency outside the two network crates.
NET_CRATES='reqwest|ureq|hyper|hyper-util|isahc|curl|attohttpc|surf|tokio-tungstenite|tungstenite|zarrs_http'
hits=$(find crates -name Cargo.toml -not -path 'crates/pe-env/*' -not -path 'crates/pe-trackers/*' \
  -not -path 'crates/pe-app/*' \
  -exec grep -nHE "^[[:space:]]*($NET_CRATES)[[:space:]]*=" {} + 2>/dev/null || true)
[ -n "$hits" ] && report "network client dependency outside pe-env and pe-trackers" "$hits"

# pe-app holds the MCP service (spec.md 3.7, D29): the one inbound exception
# to invariant 4, a loopback listener that exists only while the person has
# switched it on. It needs an HTTP *server*, never a client, so exactly this
# is admitted and nothing wider:
#   - `hyper` in [dependencies] with features from {server, http1} only;
#   - `hyper-util` in [dependencies] with features from {tokio} only;
#   - `rmcp` in [dependencies] without default features and with features
#     from {server, macros, transport-streamable-http-server} only (its
#     `auth`, `reqwest` and client transports all pull in an HTTP client);
#   - `reqwest` in [dev-dependencies] only, without default features: the
#     tests' own HTTP client, which talks to that listener on 127.0.0.1.
# The feature list is read from the dependency's own line, so each of the
# three is written on one line with its `features = [...]`; a list continued
# on the next line, or a `[dependencies.<crate>]` table, is refused rather
# than left unread.
# **This check cannot see a listener.** It reads manifests, source URLs, the
# built bundle and the CSP. That the socket is absent while the setting is
# off is held by `off_means_no_socket_and_stop_releases_the_port`
# (crates/pe-app/tests/mcp.rs), not here.
hits=$(awk -v crates="^[[:space:]]*($NET_CRATES|rmcp)[[:space:]]*=" \
  -v tables="^\\[[a-z.-]*dependencies\\.($NET_CRATES|rmcp)\\]" '
  # Whether every feature the line names is one of `allowed` (space-separated).
  function only(line, allowed,    list, parts, n, i, feature) {
    if (match(line, /features[[:space:]]*=[[:space:]]*\[[^]]*\]/) == 0) return 0
    list = substr(line, RSTART, RLENGTH)
    sub(/^[^[]*\[/, "", list); sub(/\]$/, "", list)
    n = split(list, parts, ",")
    for (i = 1; i <= n; i++) {
      feature = parts[i]; gsub(/[[:space:]"]/, "", feature)
      if (feature != "" && index(" " allowed " ", " " feature " ") == 0) return 0
    }
    return 1
  }
  /^\[/ { section = $0 }
  $0 ~ tables { print FILENAME ":" FNR ":" $0 }
  $0 ~ crates {
    name = $1
    ok = 0
    bare = ($0 ~ /default-features[[:space:]]*=[[:space:]]*false/)
    if (section == "[dependencies]" && name == "hyper" && only($0, "server http1")) ok = 1
    if (section == "[dependencies]" && name == "hyper-util" && only($0, "tokio")) ok = 1
    if (section == "[dependencies]" && name == "rmcp" && bare && only($0, "server macros transport-streamable-http-server")) ok = 1
    if (section == "[dev-dependencies]" && name == "reqwest" && bare) ok = 1
    if (section == "[dev-dependencies]" && name == "rmcp") ok = 1
    if (!ok) print FILENAME ":" FNR ":" $0
  }' crates/pe-app/Cargo.toml)
[ -n "$hits" ] && report "network client dependency in pe-app (only the MCP service's server crates are admitted)" "$hits"

# A database client only in pe-app, and only the synchronous PostgreSQL one its
# read-only metadata download uses (library::database, asked 2026-10-06).
hits=$(find crates tools -name Cargo.toml -not -path 'crates/pe-app/*' \
  -exec grep -nHE '^[[:space:]]*(postgres|tokio-postgres|tokio-postgres-rustls|sqlx|diesel)[[:space:]]*=' {} + 2>/dev/null || true)
hits="$hits$(grep -nHE '^[[:space:]]*(tokio-postgres|sqlx|diesel)[[:space:]]*=' crates/pe-app/Cargo.toml 2>/dev/null || true)"
[ -n "$hits" ] && report "database client dependency (only pe-app's postgres is admitted)" "$hits"

# C libraries the Windows ARM64 and cross-compiled builds cannot rely on
# (spec.md 1.3, CLAUDE.md): rustls with ring instead of OpenSSL or native-tls,
# our own Blosc decoder instead of blosc-src. The lockfile lists every
# platform's dependencies, so this holds for all five targets at once.
if [ -f Cargo.lock ]; then
  hits=$(grep -nE '^name = "(openssl|openssl-sys|native-tls|blosc-src|blosc-sys|zarrs_http)"' Cargo.lock || true)
  [ -n "$hits" ] && report "forbidden dependency in Cargo.lock" "$hits"
fi

# --- 3. Built bundle: remote resource references only ------------------------
if [ -d ui/dist ]; then
  # <script src="http...">, <link href="http...">, and CSS url(http...)/@import.
  hits=$(grep -rnE '(src|href)[[:space:]]*=[[:space:]]*"https?://|url\([[:space:]]*["'"'"']?https?://|@import[^;]*https?://' \
    ui/dist 2>/dev/null | grep -vE "$ALLOW" || true)
  [ -n "$hits" ] && report "remote resource reference in built bundle" "$hits"

  # A websocket is the one network call that no CSP default-src catches loosely.
  hits=$(grep -rnoE 'wss?://[A-Za-z0-9.-]+' ui/dist 2>/dev/null | grep -vE "$ALLOW" || true)
  [ -n "$hits" ] && report "websocket URL in built bundle" "$hits"
fi

# --- 4. CSP: the actual runtime enforcement ----------------------------------
csp=$(grep -o '"csp"[^,]*' "$CONF" 2>/dev/null || true)
if [ -z "$csp" ]; then
  report "tauri.conf.json" "no CSP is declared"
elif ! echo "$csp" | grep -q "default-src 'self'"; then
  report "CSP" "default-src must be 'self', got: $csp"
elif echo "$csp" | grep -qE "\*|https://" ; then
  report "CSP" "allows a wildcard or remote origin: $csp"
fi

if [ "$fail" -eq 0 ]; then
  echo "offline check passed: HTTP only in pe-env/pe-trackers, the read-only PostgreSQL client only in pe-app, CSP is 'self'-only"
fi
exit "$fail"
