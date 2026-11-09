# Vulkan loader (vulkan-1.dll)

`libmpv-2.dll` imports `vulkan-1.dll`. PCs without a GPU driver do not have it, so the release
scripts ship this copy next to `libmpv-2.dll` in every Windows installer and portable exe.

- `x64/vulkan-1.dll`   built from KhronosGroup/Vulkan-Loader commit f61c41f (Release, x64)
- License: Apache-2.0 (`LICENSE.txt`)

Rebuild (Visual Studio Build Tools, run in PowerShell):

    git clone https://github.com/KhronosGroup/Vulkan-Loader; cd Vulkan-Loader
    cmake -S . -B build -A x64 -D UPDATE_DEPS=ON
    cmake --build build --config Release --target vulkan
    # result: build\loader\Release\vulkan-1.dll

The Windows build script stops if the file is missing or is not an x64 file.
