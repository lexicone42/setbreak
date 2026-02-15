#!/bin/bash
# Quick check on overnight analysis progress
DB="$HOME/.local/share/setbreak/setbreak.db"

TOTAL=$(sqlite3 "$DB" "SELECT COUNT(*) FROM tracks;")
ANALYZED=$(sqlite3 "$DB" "SELECT COUNT(*) FROM analysis_results;")
REMAINING=$((TOTAL - ANALYZED))
PCT=$(echo "scale=1; $ANALYZED * 100 / $TOTAL" | bc)

echo "Analyzed: $ANALYZED / $TOTAL ($PCT%)"
echo "Remaining: $REMAINING"
echo ""

# Check if analysis is still running
if pgrep -f "setbreak analyze" > /dev/null; then
    echo "Status: RUNNING (PID $(pgrep -f 'setbreak analyze'))"
else
    echo "Status: NOT RUNNING"
fi

# Recent log tail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOG=$(ls -t "$SCRIPT_DIR"/logs/analyze_*.log 2>/dev/null | head -1)
if [ -n "$LOG" ]; then
    echo ""
    echo "Recent log ($LOG):"
    tail -5 "$LOG"
fi
