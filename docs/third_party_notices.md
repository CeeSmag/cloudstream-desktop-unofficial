# Third-party notices

## Twemoji Country Flags (flag emoji font)

The product Slint UI does **not** bundle this font. It remains only in the archived Tauri
frontend (`archive/desktop_tauri/`), which used the npm package `country-flag-emoji-polyfill`
(MIT) so Windows could show flag emoji in extension and repository names (for example 🇮🇶)
instead of two letters.

- Graphics: Twemoji, Copyright 2019 Twitter, Inc and other contributors, licensed under
  [CC-BY 4.0](https://creativecommons.org/licenses/by/4.0/).
- Font build: Twemoji Country Flags by Mozilla, licensed under the Apache License 2.0 / CC-BY 4.0.

## Vulkan loader

- Vulkan loader (`scripts/vendor/vulkan/x64/vulkan-1.dll`): Copyright The Khronos Group Inc. and
  contributors, licensed under the Apache License 2.0 (`scripts/vendor/vulkan/LICENSE.txt`).
  Built from [KhronosGroup/Vulkan-Loader](https://github.com/KhronosGroup/Vulkan-Loader)
  (see `scripts/vendor/vulkan/README.md`). Shipped next to libmpv in Windows packages.
