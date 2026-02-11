@echo off
REM Build script that sets up MSVC environment for Hive Rust project
REM VS 18 Enterprise MSVC 14.50.35717 cl.exe + ScopeCppSDK vc15 headers + Windows SDK 10.0.22621.0
REM
REM The MSVC C++ Desktop workload is not installed, so we use the ScopeCppSDK
REM headers (which include vcruntime.h, stdbool.h, etc.) as a substitute.

set SCOPE_SDK=C:\Program Files\Microsoft Visual Studio\18\Enterprise\SDK\ScopeCppSDK\vc15\VC
set SDK_ROOT=C:\Program Files (x86)\Windows Kits\10
set SDK_VER=10.0.22621.0
set MSVC_BIN=C:\Program Files\Microsoft Visual Studio\18\Enterprise\VC\Tools\MSVC\14.50.35717

REM INCLUDE: ScopeCppSDK headers (vcruntime.h, stdbool.h, stdint.h, etc.) + Windows SDK headers
set INCLUDE=%SCOPE_SDK%\include;%SDK_ROOT%\Include\%SDK_VER%\ucrt;%SDK_ROOT%\Include\%SDK_VER%\shared;%SDK_ROOT%\Include\%SDK_VER%\um;%SDK_ROOT%\Include\%SDK_VER%\winrt

REM LIB: ScopeCppSDK libs (msvcrt.lib, vcruntime.lib, etc.) + Windows SDK libs
set LIB=%SCOPE_SDK%\lib;%SDK_ROOT%\Lib\%SDK_VER%\ucrt\x64;%SDK_ROOT%\Lib\%SDK_VER%\um\x64

REM PATH: MSVC cl.exe + link.exe + cargo
set PATH=%MSVC_BIN%\bin\HostX64\x64;C:\Users\pat\.cargo\bin;%PATH%

cd /d H:\WORK\AG\AIrglowStudio\hive
cargo %*
