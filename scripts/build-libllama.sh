#!/usr/bin/env bash
# Builds the llama.cpp STATIC libraries (libllama + ggml) from source at the pinned commit, to be linked INTO the Veyra process. No prebuilt binary is
# downloaded and no server executable is built or shipped.
# usage: scripts/build-libllama.sh <work-dir> <out-dir>   ->  <out-dir>/include, <out-dir>/lib, <out-dir>/licenses
set -euo pipefail
PIN_TAG="b12336"; PIN_COMMIT="cfb9d4b754616ca47d165fa1ff8a7a80c30847e3"
work="$1"; out="$2"; mkdir -p "$work" "$out"; cd "$work"
git init -q src && cd src && git remote add origin https://github.com/ggml-org/llama.cpp
git fetch -q --depth 1 origin "$PIN_COMMIT" && git checkout -q FETCH_HEAD
[ "$(git rev-parse HEAD)" = "$PIN_COMMIT" ] || { echo "commit mismatch"; exit 1; }
args=(-DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DGGML_NATIVE=OFF -DGGML_OPENMP=OFF -DLLAMA_OPENSSL=OFF
  -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF
  -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_APP=OFF -DLLAMA_BUILD_SERVER=OFF -DLLAMA_BUILD_TOOLS=OFF -DLLAMA_BUILD_COMMON=OFF)
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  args+=(-DCMAKE_OSX_ARCHITECTURES=arm64  -DCMAKE_OSX_DEPLOYMENT_TARGET=12.0 -DGGML_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON) ;;
  Darwin-x86_64) args+=(-DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET=12.0 -DGGML_METAL=OFF -DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON) ;;
  *)             args+=(-DGGML_AVX2=ON -DGGML_FMA=ON -DGGML_F16C=ON) ;;   # x86-64 with AVX2 (Haswell+, includes the i5-8350U)
esac
case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) args+=(-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded -DCMAKE_POLICY_DEFAULT_CMP0091=NEW) ;; esac  # static CRT: no VC++ redistributable needed
cmake -S . -B build "${args[@]}"
cmake --build build --config Release --target llama -j"$( (nproc || sysctl -n hw.ncpu) 2>/dev/null || echo 2)"
mkdir -p "$out/include" "$out/lib" "$out/licenses"
cp include/llama.h ggml/include/*.h "$out/include/"
find build \( -name 'libllama.a' -o -name 'libggml*.a' -o -name 'llama.lib' -o -name 'ggml*.lib' \) -type f -exec cp {} "$out/lib/" \;
cp LICENSE "$out/licenses/LLAMA_CPP_LICENSE"; cp licenses/LICENSE-* "$out/licenses/" 2>/dev/null || true
echo "$PIN_TAG $PIN_COMMIT" > "$out/LLAMA_CPP_VERSION"
find "$out" -type f | sort
