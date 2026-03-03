#!/bin/bash
# Overnight analysis: waits until 11pm, runs until 8am, then stops gracefully.
# Usage: nohup ./overnight_analyze_11pm.sh > logs/overnight_analyze_11pm.out 2>&1 &

set -euo pipefail

SETBREAK="./target/release/setbreak"
DB_PATH="$HOME/.local/share/setbreak/setbreak.db"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"
JOBS=4

mkdir -p "$LOG_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/analyze_${TIMESTAMP}.log"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

# --- Wait until 11pm ---
TARGET="today 23:00:00"
NOW=$(date +%s)
THEN=$(date -d "$TARGET" +%s)
DELAY=$((THEN - NOW))

if [ "$DELAY" -le 0 ]; then
    log "It's already past 11pm, starting immediately"
else
    log "Waiting $((DELAY / 60)) minutes until 11pm..."
    sleep "$DELAY"
fi

# --- Pre-run stats ---
log "========================================"
log "SetBreak Overnight Analysis"
log "Workers: $JOBS"
log "Stop time: 8am"
log "DB: $DB_PATH"
log "Log: $LOG_FILE"
log "========================================"

UNANALYZED=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM tracks t LEFT JOIN analysis_results ar ON t.id = ar.track_id WHERE ar.track_id IS NULL;")
log "Tracks remaining to analyze: $UNANALYZED"

# --- Launch analysis in background ---
"$SETBREAK" analyze -j "$JOBS" --db-path "$DB_PATH" -v >> "$LOG_FILE" 2>&1 &
ANALYZE_PID=$!
log "Analysis started (PID $ANALYZE_PID)"

# --- Schedule 8am kill ---
STOP_TIME="tomorrow 08:00:00"
STOP_NOW=$(date +%s)
STOP_THEN=$(date -d "$STOP_TIME" +%s)
STOP_DELAY=$((STOP_THEN - STOP_NOW))

log "Will stop in $((STOP_DELAY / 3600)) hours at 8am"

(
    sleep "$STOP_DELAY"
    if kill -0 "$ANALYZE_PID" 2>/dev/null; then
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] 8am — sending SIGINT for graceful shutdown" >> "$LOG_FILE"
        kill -INT "$ANALYZE_PID"
        # Give it 60s to finish current chunk and write to DB
        sleep 60
        if kill -0 "$ANALYZE_PID" 2>/dev/null; then
            echo "[$(date '+%Y-%m-%d %H:%M:%S')] Still running after 60s, sending SIGTERM" >> "$LOG_FILE"
            kill -TERM "$ANALYZE_PID"
        fi
    fi
) &
TIMER_PID=$!

# --- Wait for analysis to finish (either naturally or via 8am kill) ---
wait "$ANALYZE_PID" 2>/dev/null
EXIT_CODE=$?

# Clean up timer if analysis finished early
kill "$TIMER_PID" 2>/dev/null || true
wait "$TIMER_PID" 2>/dev/null || true

# --- Post-run stats ---
log "========================================"
log "Analysis stopped (exit code: $EXIT_CODE)"

ANALYZED_NOW=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM analysis_results;")
REMAINING=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM tracks t LEFT JOIN analysis_results ar ON t.id = ar.track_id WHERE ar.track_id IS NULL;")
DONE_THIS_RUN=$((UNANALYZED - REMAINING))

log "Analyzed this run: $DONE_THIS_RUN"
log "Total analyzed: $ANALYZED_NOW"
log "Remaining: $REMAINING"
log "Finished: $(date)"
log "========================================"

# --- Extract boundaries for newly analyzed tracks ---
log "Extracting boundary features for new tracks..."
"$SETBREAK" extract-boundaries -j "$JOBS" --db-path "$DB_PATH" >> "$LOG_FILE" 2>&1 || true
log "Boundary extraction complete"

log "Full log at: $LOG_FILE"
