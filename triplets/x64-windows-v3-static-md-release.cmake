set(VCPKG_TARGET_ARCHITECTURE x64)
set(VCPKG_CRT_LINKAGE dynamic)
set(VCPKG_LIBRARY_LINKAGE static)

# Release only - skip debug builds to save time.
set(VCPKG_BUILD_TYPE release)
set(VCPKG_C_FLAGS_RELEASE "/arch:AVX2")
set(VCPKG_CXX_FLAGS_RELEASE "/arch:AVX2")
