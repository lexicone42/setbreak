#!/bin/bash
# Cron-launched overnight analysis: runs until 8am, then stops gracefully.
# Designed to be launched by cron at 11pm:
#   0 23 * * * /datar/workspace/claude_code_experiments/setbreak/overnight_cron.sh
#
# Features:
#   - 2 workers (bounded memory, safe for 96kHz FLACs)
#   - Scans for new files, then analyzes unanalyzed tracks
#   - Graceful 8am shutdown (SIGINT → 60s grace → SIGTERM)
#   - Memory monitoring every 5 minutes
#   - Boundary extraction + rescore after analysis
#   - Prevents duplicate runs via lockfile

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SETBREAK="$SCRIPT_DIR/target/release/setbreak"
DB_PATH="$HOME/.local/share/setbreak/setbreak.db"
LOG_DIR="$SCRIPT_DIR/logs"
LOCK_FILE="/tmp/setbreak-overnight.lock"
JOBS=2

# ── Prevent duplicate runs ──────────────────────────────────────────
if [ -f "$LOCK_FILE" ]; then
    OLD_PID=$(cat "$LOCK_FILE" 2>/dev/null)
    if kill -0 "$OLD_PID" 2>/dev/null; then
        echo "Already running (PID $OLD_PID), exiting."
        exit 0
    fi
    rm -f "$LOCK_FILE"
fi
echo $$ > "$LOCK_FILE"
trap 'rm -f "$LOCK_FILE"' EXIT

# ── Logging ─────────────────────────────────────────────────────────
mkdir -p "$LOG_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/analyze_${TIMESTAMP}.log"
ln -sf "$LOG_FILE" "$LOG_DIR/analyze_latest.log"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

log "========================================"
log "SetBreak Overnight Analysis (cron)"
log "Workers: $JOBS"
log "Stop time: 8am"
log "DB: $DB_PATH"
log "Log: $LOG_FILE"
log "========================================"

# ── Pre-run: scan for new files ─────────────────────────────────────
log "Scanning for new files..."
"$SETBREAK" scan --db-path "$DB_PATH" >> "$LOG_FILE" 2>&1 || true

UNANALYZED=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM tracks t LEFT JOIN analysis_results ar ON t.id = ar.track_id WHERE ar.track_id IS NULL;")
log "Tracks remaining to analyze: $UNANALYZED"

if [ "$UNANALYZED" -eq 0 ]; then
    log "Nothing to analyze. Running rescore + boundary extraction instead."
    "$SETBREAK" rescore --db-path "$DB_PATH" >> "$LOG_FILE" 2>&1 || true
    "$SETBREAK" extract-boundaries -j "$JOBS" --db-path "$DB_PATH" >> "$LOG_FILE" 2>&1 || true
    log "Done (no analysis needed)."
    exit 0
fi

# ── Launch analysis ─────────────────────────────────────────────────
RUST_LOG=info "$SETBREAK" analyze -j "$JOBS" --db-path "$DB_PATH" -v >> "$LOG_FILE" 2>&1 &
ANALYZE_PID=$!
log "Analysis started (PID $ANALYZE_PID)"

# ── Memory monitor (log RSS every 5 min) ────────────────────────────
(
    while kill -0 "$ANALYZE_PID" 2>/dev/null; do
        RSS_KB=$(ps -o rss= -p "$ANALYZE_PID" 2>/dev/null || echo "0")
        RSS_MB=$((RSS_KB / 1024))
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] Memory: ${RSS_MB} MB RSS" >> "$LOG_FILE"
        sleep 300
    done
) &
MONITOR_PID=$!

# ── Schedule 8am shutdown ───────────────────────────────────────────
STOP_TIME="tomorrow 08:00:00"
STOP_NOW=$(date +%s)
STOP_THEN=$(date -d "$STOP_TIME" +%s)
STOP_DELAY=$((STOP_THEN - STOP_NOW))

# Sanity check: if we're somehow running after 8am, stop in 1 hour max
if [ "$STOP_DELAY" -le 0 ] || [ "$STOP_DELAY" -gt 36000 ]; then
    STOP_DELAY=3600
    log "Warning: unusual time, capping run at 1 hour"
fi

log "Will stop in $((STOP_DELAY / 3600))h $((STOP_DELAY % 3600 / 60))m"

(
    sleep "$STOP_DELAY"
    if kill -0 "$ANALYZE_PID" 2>/dev/null; then
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] 8am — sending SIGINT for graceful shutdown" >> "$LOG_FILE"
        kill -INT "$ANALYZE_PID"
        sleep 60
        if kill -0 "$ANALYZE_PID" 2>/dev/null; then
            echo "[$(date '+%Y-%m-%d %H:%M:%S')] Still running after 60s, sending SIGTERM" >> "$LOG_FILE"
            kill -TERM "$ANALYZE_PID"
        fi
    fi
) &
TIMER_PID=$!

# ── Wait for analysis ──────────────────────────────────────────────
wait "$ANALYZE_PID" 2>/dev/null
EXIT_CODE=$?

kill "$TIMER_PID" 2>/dev/null || true
kill "$MONITOR_PID" 2>/dev/null || true
wait "$TIMER_PID" 2>/dev/null || true
wait "$MONITOR_PID" 2>/dev/null || true

# ── Post-run stats ──────────────────────────────────────────────────
ANALYZED_NOW=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM analysis_results;")
REMAINING=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM tracks t LEFT JOIN analysis_results ar ON t.id = ar.track_id WHERE ar.track_id IS NULL;")
DONE_THIS_RUN=$((UNANALYZED - REMAINING))

log "========================================"
log "Analysis stopped (exit code: $EXIT_CODE)"
log "Analyzed this run: $DONE_THIS_RUN"
log "Total analyzed: $ANALYZED_NOW"
log "Remaining: $REMAINING"
log "========================================"

# ── Post-analysis: boundary extraction + rescore ────────────────────
log "Extracting boundary features..."
"$SETBREAK" extract-boundaries -j "$JOBS" --db-path "$DB_PATH" >> "$LOG_FILE" 2>&1 || true

log "Rescoring..."
"$SETBREAK" rescore --db-path "$DB_PATH" >> "$LOG_FILE" 2>&1 || true

log "Done. Full log: $LOG_FILE"
