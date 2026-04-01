#!/usr/bin/env python3

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


def send_message(proc: subprocess.Popen[bytes], payload: dict) -> None:
    body = json.dumps(payload, separators=(",", ":")).encode("utf-8") + b"\n"
    assert proc.stdin is not None
    proc.stdin.write(body)
    proc.stdin.flush()


def read_message(proc: subprocess.Popen[bytes]) -> dict:
    assert proc.stdout is not None
    line = proc.stdout.readline()
    if not line:
        stderr = b""
        if proc.stderr is not None:
            stderr = proc.stderr.read()
        raise RuntimeError(
            f"MCP server closed stdout unexpectedly. stderr={stderr.decode('utf-8', errors='replace')}"
        )
    return json.loads(line)


def read_response(proc: subprocess.Popen[bytes], expected_id: int) -> dict:
    while True:
        message = read_message(proc)
        if message.get("id") == expected_id:
            return message


def main() -> int:
    binary = os.environ.get("MCP_BINARY", "codebones-mcp")
    proc = subprocess.Popen(
        [binary],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    try:
        send_message(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {"name": "codebones-smoke", "version": "0.1.0"},
                },
            },
        )
        init = read_response(proc, 1)
        if "result" not in init:
            raise RuntimeError(f"initialize failed: {init}")

        send_message(
            proc,
            {
                "jsonrpc": "2.0",
                "method": "notifications/initialized",
            },
        )

        send_message(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list",
                "params": {},
            },
        )
        tools_response = read_response(proc, 2)
        tools = tools_response.get("result", {}).get("tools", [])
        tool_names = {tool["name"] for tool in tools}
        expected = {"index", "outline", "get", "search"}
        if tool_names != expected:
            raise RuntimeError(
                f"unexpected MCP tools: got {sorted(tool_names)}, expected {sorted(expected)}"
            )

        with tempfile.TemporaryDirectory() as tmp_dir:
            repo_dir = Path(tmp_dir)
            repo_dir.joinpath("lib.rs").write_text(
                "pub fn compat_mode() -> &'static str { \"ok\" }\n",
                encoding="utf-8",
            )

            send_message(
                proc,
                {
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": {
                        "name": "index",
                        "arguments": {
                            "dir": str(repo_dir),
                        },
                    },
                },
            )
            index_response = read_response(proc, 3)
            index_status = (
                index_response.get("result", {})
                .get("structuredContent", {})
                .get("status")
            )
            if index_status != "indexed":
                raise RuntimeError(f"index tool failed: {index_response}")

            send_message(
                proc,
                {
                    "jsonrpc": "2.0",
                    "id": 4,
                    "method": "tools/call",
                    "params": {
                        "name": "search",
                        "arguments": {
                            "dir": str(repo_dir),
                            "query": "compat",
                        },
                    },
                },
            )
            search_response = read_response(proc, 4)
            results = (
                search_response.get("result", {})
                .get("structuredContent", {})
                .get("results", [])
            )
            if "lib.rs::compat_mode" not in results:
                raise RuntimeError(f"search tool failed: {search_response}")

        return 0
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
