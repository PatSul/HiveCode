#!/bin/bash
# Build script that sets up MSVC environment
# VS 2022 Enterprise MSVC 14.44.35207 + ScopeCppSDK vc15 headers + Windows SDK 10.0.22621.0

MSVC_BIN="C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\VC\\Tools\\MSVC\\14.44.35207"
SCOPE_SDK="C:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\SDK\\ScopeCppSDK\\vc15\\VC"
SDK_ROOT="C:\\Program Files (x86)\\Windows Kits\\10"
SDK_VER="10.0.22621.0"

# LIB: ScopeCppSDK libs + Windows SDK libs
export LIB="${SCOPE_SDK}\\lib;${SDK_ROOT}\\Lib\\${SDK_VER}\\um\\x64;${SDK_ROOT}\\Lib\\${SDK_VER}\\ucrt\\x64"

# INCLUDE: ScopeCppSDK headers (has vcruntime.h + STL) + Windows SDK headers
export INCLUDE="${SCOPE_SDK}\\include;${SDK_ROOT}\\Include\\${SDK_VER}\\ucrt;${SDK_ROOT}\\Include\\${SDK_VER}\\um;${SDK_ROOT}\\Include\\${SDK_VER}\\shared"

# PATH: MSVC binaries (cl.exe, link.exe)
export PATH="${MSVC_BIN}\\bin\\Hostx64\\x64:${PATH}"

cd "H:/WORK/AG/AIrglowStudio/hive"
/c/Users/pat/.cargo/bin/cargo.exe "$@"
