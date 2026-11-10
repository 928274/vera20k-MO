# MSVC 2022 BuildTools desktop toolchain (the VS 2026/18 one is OneCore-only:
# no include/, no lib/x64, so linking fails with LNK1104: msvcrt.lib).
# Source this before any cargo build on Windows.
MSVC='/c/Program Files (x86)/Microsoft Visual Studio/2022/BuildTools/VC/Tools/MSVC/14.44.35207'
SDK='/c/Program Files (x86)/Windows Kits/10'
SDKVER='10.0.26100.0'
RUSTBIN='/c/Users/92827/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin'
export PATH="$MSVC/bin/HostX64/x64:$SDK/bin/$SDKVER/x64:$RUSTBIN:$PATH"
export INCLUDE='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\shared;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\winrt'
export LIB='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64'
