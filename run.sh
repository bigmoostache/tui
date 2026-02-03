#!/bin/bash
# Supervisor script for TUI - handles reload requests

STATE_FILE=".context-pilot/state.json"

while true; do
    # Run the TUI
    cargo run --release -- "$@"
    
    # Check if reload was requested
    if [ -f "$STATE_FILE" ]; then
        RELOAD=$(grep -o '"reload_requested":\s*true' "$STATE_FILE" 2>/dev/null)
        if [ -n "$RELOAD" ]; then
            echo "Reload requested, restarting..."
            # Small delay to ensure file is fully written
            sleep 0.2
            # Add --resume-stream if not already present
            if [[ ! " $* " =~ " --resume-stream " ]]; then
                set -- "$@" --resume-stream
            fi
            continue
        fi
    fi
    
    # No reload requested, exit
    break
done
