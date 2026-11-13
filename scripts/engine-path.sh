#!/usr/bin/env bash
# prints the path of the engine executable inside a directory (llama-server.exe on Windows, llama-server elsewhere); fails clearly if absent
d="$1"; if [ -f "$d/llama-server.exe" ]; then echo "$d/llama-server.exe"; elif [ -f "$d/llama-server" ]; then echo "$d/llama-server"; else echo "no engine executable in $d:" >&2; ls -la "$d" >&2; exit 1; fi
