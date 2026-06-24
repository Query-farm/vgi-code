#!/usr/bin/env bash
# Copyright 2026 Query Farm LLC - https://query.farm
#
# Run this repo's sqllogictest suite (test/sql/*.test) against the vgi-code
# VGI worker, using a prebuilt standalone `haybarn-unittest` and the signed
# community `vgi` extension — no C++ build from source. See ci/README.md.
#
# Parameterized by TRANSPORT (default: subprocess), exercising the SAME suite
# over each transport the vgi extension supports — the only thing that changes
# is what LOCATION (VGI_CODE_WORKER) the .test files ATTACH:
#
#   subprocess  VGI_CODE_WORKER = the stdio worker command (DuckDB spawns it).
#   http        start `code-worker --http` (auto port; advertises `PORT:<n>`
#               on stdout), VGI_CODE_WORKER = http://127.0.0.1:<port>.
#   unix        start `code-worker --unix <sock>` (advertises `UNIX:<sock>`
#               on stdout), VGI_CODE_WORKER = unix://<sock>.
#
# Required environment:
#   HAYBARN_UNITTEST  path to the haybarn-unittest binary
#   WORKER_BIN        path to the compiled code-worker binary (used to launch
#                     the http/unix servers, and the stdio LOCATION). Falls back
#                     to VGI_CODE_WORKER when that is a bare command (subprocess).
# Optional:
#   TRANSPORT         subprocess | http | unix   (default: subprocess)
#   STAGE             scratch dir for the preprocessed test tree (default: mktemp)
set -euo pipefail

TRANSPORT="${TRANSPORT:-subprocess}"

: "${HAYBARN_UNITTEST:?path to the haybarn-unittest binary}"

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
STAGE="${STAGE:-$(mktemp -d)}"

# For http/unix we must launch the worker binary ourselves; for subprocess the
# binary IS the LOCATION. WORKER_BIN names the compiled binary; default to the
# release build in this repo.
WORKER_BIN="${WORKER_BIN:-$REPO/target/release/code-worker}"

SERVER_PID=""
SOCK_PATH=""
cleanup() {
  local rc=$?
  if [[ -n "$SERVER_PID" ]]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  [[ -n "$SOCK_PATH" ]] && rm -f "$SOCK_PATH" 2>/dev/null || true
  return "$rc"
}
trap cleanup EXIT

# Bring up the out-of-band server for http/unix and resolve VGI_CODE_WORKER.
# Both transports announce their endpoint on stdout (`PORT:<n>` / `UNIX:<path>`),
# which we poll for in the log before running the suite (readiness gate). The
# worker is launched with cwd = the stage dir so it resolves any staged
# relative-path fixtures identically to the subprocess (DuckDB-spawned) case.
start_server_and_set_location() {
  local kind="$1"
  : "${WORKER_BIN:?path to the code-worker binary (WORKER_BIN)}"
  [[ -x "$WORKER_BIN" ]] || { echo "ERROR: worker binary not executable: $WORKER_BIN" >&2; exit 1; }

  local log="$STAGE/.worker-$kind.log"
  case "$kind" in
    http)
      ( cd "$STAGE" && exec "$WORKER_BIN" --http ) >"$log" 2>&1 &
      SERVER_PID=$!
      local port=""
      for _ in $(seq 1 60); do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
          echo "ERROR: worker (--http) exited during startup. Log:" >&2; cat "$log" >&2; exit 1
        fi
        port=$(sed -n 's/.*PORT:\([0-9][0-9]*\).*/\1/p' "$log" 2>/dev/null | head -1)
        [[ -n "$port" ]] && break
        sleep 0.5
      done
      [[ -n "$port" ]] || { echo "ERROR: timed out waiting for PORT:<n>. Log:" >&2; cat "$log" >&2; exit 1; }
      export VGI_CODE_WORKER="http://127.0.0.1:$port"
      echo "HTTP worker ready on 127.0.0.1:$port (pid $SERVER_PID)"
      ;;
    unix)
      SOCK_PATH="${VGI_CODE_SOCK:-/tmp/code.$$.sock}"
      rm -f "$SOCK_PATH" 2>/dev/null || true
      ( cd "$STAGE" && exec "$WORKER_BIN" --unix "$SOCK_PATH" ) >"$log" 2>&1 &
      SERVER_PID=$!
      local ready=""
      for _ in $(seq 1 60); do
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
          echo "ERROR: worker (--unix) exited during startup. Log:" >&2; cat "$log" >&2; exit 1
        fi
        if grep -q "UNIX:$SOCK_PATH" "$log" 2>/dev/null && [[ -S "$SOCK_PATH" ]]; then
          ready=1; break
        fi
        sleep 0.5
      done
      [[ -n "$ready" ]] || { echo "ERROR: timed out waiting for UNIX socket. Log:" >&2; cat "$log" >&2; exit 1; }
      export VGI_CODE_WORKER="unix://$SOCK_PATH"
      echo "Unix worker ready on $SOCK_PATH (pid $SERVER_PID)"
      ;;
  esac
}

echo "Staging preprocessed tests into $STAGE ..."
mkdir -p "$STAGE/test/sql"
# Pass the transport to the preprocessor: the http leg additionally needs DuckDB's
# `httpfs` extension loaded (the vgi extension's HTTP client is built on it), so
# the awk injects a signed INSTALL/LOAD httpfs after each `LOAD vgi;`. Without it
# the http ATTACH fails with a "HTTP"-containing error that the sqllogictest
# runner *silently auto-skips* (default ignore_error_messages), masking the gap.
for f in "$REPO"/test/sql/*.test; do
  awk -v transport="$TRANSPORT" -f "$HERE/preprocess-require.awk" "$f" > "$STAGE/test/sql/$(basename "$f")"
done


# Now that the stage dir holds the fixtures, bring up the out-of-band server for
# http/unix (started with cwd = STAGE) and resolve the LOCATION. For subprocess
# the binary itself is the stdio LOCATION DuckDB spawns.
case "$TRANSPORT" in
  subprocess) export VGI_CODE_WORKER="${VGI_CODE_WORKER:-$WORKER_BIN}" ;;
  http)  start_server_and_set_location http ;;
  unix)  start_server_and_set_location unix ;;
  *) echo "ERROR: unknown TRANSPORT '$TRANSPORT' (want subprocess|http|unix)" >&2; exit 1 ;;
esac

: "${VGI_CODE_WORKER:?worker LOCATION (stdio command, http:// URL, or unix:// socket)}"

cd "$STAGE"

# Warm the extension cache once: vgi from the signed community channel. A miss
# here is only a warning — the per-test LOAD vgi; (the .test files load it
# explicitly) is what actually gates each file, and that LOAD only succeeds once
# vgi has been INSTALLed from community.
echo "Warming the extension cache (vgi from community) ..."
mkdir -p "$STAGE/test"
cat > "$STAGE/test/_warm.test" <<'WARM'
# name: test/_warm.test
# group: [warm]
statement ok
INSTALL vgi FROM community;
WARM
"$HAYBARN_UNITTEST" "test/_warm.test" >/dev/null 2>&1 || echo "::warning::extension warm step did not fully succeed"
rm -f "$STAGE/test/_warm.test"

# Run the whole suite in one invocation, streaming the runner's native
# sqllogictest report. Any failed assertion exits non-zero and fails the job.
#
# Guard against the silent-skip trap: DuckDB's sqllogictest runner auto-skips
# any test whose error message contains "HTTP" (default ignore_error_messages),
# so a broken http leg can report "All tests were skipped" with exit 0 and look
# green. Tee the report and fail if NOTHING actually ran. (For subprocess/unix
# there is no skip path, so this only ever bites a genuinely broken http leg.)
echo "Running suite (transport: $TRANSPORT, worker: $VGI_CODE_WORKER) ..."
REPORT="$STAGE/.report.txt"
set +e
"$HAYBARN_UNITTEST" "test/sql/*" 2>&1 | tee "$REPORT"
status="${PIPESTATUS[0]}"
set -e
if grep -qiE "All tests were skipped|total skipped [1-9]" "$REPORT"; then
  echo "ERROR: tests were SKIPPED — almost certainly an ATTACH/transport error whose" >&2
  echo "       message matched the runner's default ignore list (e.g. \"HTTP\"). A skip" >&2
  echo "       is NOT a pass. Transport=$TRANSPORT worker=$VGI_CODE_WORKER." >&2
  exit 1
fi
exit "$status"
