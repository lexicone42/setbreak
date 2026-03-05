#!/bin/bash
# Run analysis with 2 workers. 96kHz files auto-downsampled to 48kHz.
# Usage: nohup ./run_analysis.sh &
# Check: tail -f logs/analyze_latest.log | grep -E "Chunk|Memory"
# Stop:  kill $(cat logs/analyze.pid)

cd "$(dirname "$0")"
mkdir -p logs
LOG="logs/analyze_$(date +%Y%m%d_%H%M%S).log"
ln -sf "$(basename "$LOG")" logs/analyze_latest.log

echo "[$(date)] Starting analysis (2 workers)" | tee "$LOG"
echo "$$" > logs/analyze.pid

RUST_LOG=info ./target/release/setbreak analyze -j 2 \
    --db-path "$HOME/.local/share/setbreak/setbreak.db" -v >> "$LOG" 2>&1
RC=$?

echo "[$(date)] Analysis finished (exit: $RC)" | tee -a "$LOG"

if [ $RC -eq 0 ]; then
    echo "[$(date)] Extracting boundaries..." | tee -a "$LOG"
    RUST_LOG=info ./target/release/setbreak extract-boundaries -j 2 \
        --db-path "$HOME/.local/share/setbreak/setbreak.db" >> "$LOG" 2>&1 || true
    echo "[$(date)] Boundary extraction complete" | tee -a "$LOG"
fi

echo "[$(date)] Done. Log: $LOG" | tee -a "$LOG"
