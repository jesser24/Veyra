#!/usr/bin/env bash
# Builds llama-server from source at the pinned commit. No prebuilt executable is ever downloaded.
# usage: scripts/build-llama-server.sh <work-dir> <out-dir>
set -euo pipefail
PIN_TAG="b12336"; PIN_COMMIT="cfb9d4b754616ca47d165fa1ff8a7a80c30847e3"
work="$1"; out="$2"; mkdir -p "$work" "$out"; cd "$work"
git init -q src && cd src && git remote add origin https://github.com/ggml-org/llama.cpp
git fetch -q --depth 1 origin "$PIN_COMMIT" && git checkout -q FETCH_HEAD
[ "$(git rev-parse HEAD)" = "$PIN_COMMIT" ] || { echo "commit mismatch"; exit 1; }
args=(-DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DGGML_NATIVE=OFF -DGGML_OPENMP=OFF -DLLAMA_OPENSSL=OFF
  -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF   # the prebuilt web UI would be an unverified download; Veyra has its own UI
  -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_APP=OFF -DLLAMA_BUILD_SERVER=ON -DLLAMA_BUILD_TOOLS=ON)
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  args+=(-DCMAKE_OSX_ARCHITECTURES=arm64  -DCMAKE_OSX_DEPLOYMENT_TARGET=12.0 -DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON) ;;
  Darwin-x86_64) args+=(-DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET=12.0 -DGGML_METAL=OFF -DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON) ;;
  *)             args+=(-DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON) ;;   # x86-64 with AVX2 (Haswell+, includes the i5-8350U)
esac
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) args+=(-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DCMAKE_POLICY_DEFAULT_CMP0091=NEW) ;; esac  # static CRT: no VC++ redistributable needed
cmake -S . -B build "${args[@]}"
cmake --build build --config Release --target llama-server -j"$( (nproc || sysctl -n hw.ncpu) 2>/dev/null || echo 2)"
echo "candidates:"; find build -name 'llama-server*' -not -path '*/CMakeFiles/*' | head -20
bin=$(find build -type f -name 'llama-server.exe' | head -n1)
[ -n "$bin" ] || bin=$(find build -type f -name 'llama-server' -not -path '*/CMakeFiles/*' | head -n1)
[ -n "$bin" ] || { echo "llama-server not built"; exit 1; }
echo "using $bin"
cp "$bin" "$out/"; cp LICENSE "$out/LLAMA_CPP_LICENSE"; cp licenses/LICENSE-* "$out/" 2>/dev/null || true
echo "$PIN_TAG $PIN_COMMIT" > "$out/LLAMA_CPP_VERSION"
ls -la "$out"
