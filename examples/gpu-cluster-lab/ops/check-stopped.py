import json
import sys


def validate(state):
    return (
        isinstance(state, dict)
        and state.get("Status") == "exited"
        and type(state.get("ExitCode")) is int
        and state["ExitCode"] == 0
        and state.get("Error") == ""
        and all(
            state.get(flag) is False
            for flag in ("Running", "Paused", "Restarting", "OOMKilled", "Dead")
        )
    )


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("Usage: check-stopped.py <service> < docker-state.json")
    state = json.load(sys.stdin)
    if not validate(state):
        sys.exit(f"{sys.argv[1]} did not stop gracefully: {json.dumps(state)}")
    print(f"{sys.argv[1]} stopped gracefully (exit 0).")
