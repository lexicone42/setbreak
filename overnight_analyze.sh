#!/bin/bash
# SetBreak overnight analysis script
# Runs analysis with progress logging, handles interrupts gracefully.
# Usage: nohup ./overnight_analyze.sh &           # incremental (new tracks only)
#        FORCE=1 nohup ./overnight_analyze.sh &   # full rescan (all tracks)

set -euo pipefail

SETBREAK="./target/release/setbreak"
DB_PATH="$HOME/.local/share/setbreak/setbreak.db"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG_DIR="$SCRIPT_DIR/logs"
JOBS=4  # 4 of 6 cores — leaving room for the system
MAX_HOURS=8

mkdir -p "$LOG_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/analyze_${TIMESTAMP}.log"
STATS_FILE="$LOG_DIR/analyze_${TIMESTAMP}_stats.log"

exec > >(tee -a "$LOG_FILE") 2>&1

echo "========================================"
echo "SetBreak Overnight Analysis"
echo "Started: $(date)"
echo "Workers: $JOBS"
echo "Max hours: $MAX_HOURS"
echo "DB: $DB_PATH"
echo "Log: $LOG_FILE"
echo "========================================"
echo ""

# Pre-run stats
echo "--- Pre-run stats ---"
"$SETBREAK" stats --db-path "$DB_PATH"
echo ""

UNANALYZED=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM tracks t LEFT JOIN analysis_results ar ON t.id = ar.track_id WHERE ar.track_id IS NULL;")
echo "Tracks remaining to analyze: $UNANALYZED"
echo ""

START_TIME=$SECONDS

# Run analysis — the progress bar goes to stderr (tee captures both)
# The --verbose flag gives us INFO-level logging of individual track results
# Use --force for full rescan (re-analyze all tracks), omit for incremental
FORCE_FLAG="${FORCE:+--force}"
"$SETBREAK" analyze -j "$JOBS" --db-path "$DB_PATH" $FORCE_FLAG -v 2>&1

EXIT_CODE=$?
ELAPSED=$(( SECONDS - START_TIME ))
HOURS=$(echo "scale=2; $ELAPSED / 3600" | bc)

echo ""
echo "========================================"
echo "Analysis finished"
echo "Exit code: $EXIT_CODE"
echo "Elapsed: ${HOURS} hours (${ELAPSED} seconds)"
echo "Finished: $(date)"
echo "========================================"
echo ""

# Post-run stats
echo "--- Post-run stats ---"
"$SETBREAK" stats --db-path "$DB_PATH"

ANALYZED_NOW=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM analysis_results;")
echo ""
echo "Tracks analyzed total: $ANALYZED_NOW"

# Score distribution summary
echo ""
echo "--- Score distributions (mean / min / max) ---"
sqlite3 "$DB_PATH" "
SELECT
  'energy' as score, ROUND(AVG(energy_score),1), MIN(energy_score), MAX(energy_score) FROM analysis_results WHERE energy_score IS NOT NULL
UNION ALL SELECT
  'intensity', ROUND(AVG(intensity_score),1), MIN(intensity_score), MAX(intensity_score) FROM analysis_results WHERE intensity_score IS NOT NULL
UNION ALL SELECT
  'groove', ROUND(AVG(groove_score),1), MIN(groove_score), MAX(groove_score) FROM analysis_results WHERE groove_score IS NOT NULL
UNION ALL SELECT
  'improv', ROUND(AVG(improvisation_score),1), MIN(improvisation_score), MAX(improvisation_score) FROM analysis_results WHERE improvisation_score IS NOT NULL
UNION ALL SELECT
  'tightness', ROUND(AVG(tightness_score),1), MIN(tightness_score), MAX(tightness_score) FROM analysis_results WHERE tightness_score IS NOT NULL
UNION ALL SELECT
  'build_q', ROUND(AVG(build_quality_score),1), MIN(build_quality_score), MAX(build_quality_score) FROM analysis_results WHERE build_quality_score IS NOT NULL
UNION ALL SELECT
  'exploratory', ROUND(AVG(exploratory_score),1), MIN(exploratory_score), MAX(exploratory_score) FROM analysis_results WHERE exploratory_score IS NOT NULL
UNION ALL SELECT
  'transcend', ROUND(AVG(transcendence_score),1), MIN(transcendence_score), MAX(transcendence_score) FROM analysis_results WHERE transcendence_score IS NOT NULL
UNION ALL SELECT
  'valence', ROUND(AVG(valence_score),1), MIN(valence_score), MAX(valence_score) FROM analysis_results WHERE valence_score IS NOT NULL
UNION ALL SELECT
  'arousal', ROUND(AVG(arousal_score),1), MIN(arousal_score), MAX(arousal_score) FROM analysis_results WHERE arousal_score IS NOT NULL
;"

echo ""
echo "Full log at: $LOG_FILE"
