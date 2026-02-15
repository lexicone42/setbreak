#!/bin/bash
# Finish analyzing remaining unanalyzed tracks.
# Does NOT use --force — only processes tracks missing from analysis_results.
# Usage: nohup ./finish_analyze.sh &
# Or just: ./finish_analyze.sh (if you want to watch it)

set -euo pipefail

SETBREAK="./target/release/setbreak"
DB_PATH="$HOME/.local/share/setbreak/setbreak.db"
LOG_DIR="/datar/workspace/claude_code_experiments/setbreak/logs"
JOBS=4  # 4 of 6 cores

mkdir -p "$LOG_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/finish_${TIMESTAMP}.log"

exec > >(tee -a "$LOG_FILE") 2>&1

TOTAL=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM tracks;")
ANALYZED=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM analysis_results;")
REMAINING=$((TOTAL - ANALYZED))

echo "========================================"
echo "SetBreak — Finish Analysis"
echo "Started: $(date)"
echo "Workers: $JOBS"
echo "DB: $DB_PATH"
echo "Log: $LOG_FILE"
echo "========================================"
echo ""
echo "Progress so far: $ANALYZED / $TOTAL analyzed ($REMAINING remaining)"
echo ""

START_TIME=$SECONDS

# No --force: only analyzes tracks without existing results
"$SETBREAK" analyze -j "$JOBS" --db-path "$DB_PATH" -v 2>&1

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
ANALYZED_NOW=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM analysis_results;")
NEWLY_DONE=$((ANALYZED_NOW - ANALYZED))
echo "Newly analyzed: $NEWLY_DONE"
echo "Total analyzed: $ANALYZED_NOW / $TOTAL"
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
