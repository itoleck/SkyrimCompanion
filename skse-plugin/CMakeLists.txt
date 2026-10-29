cmake_minimum_required(VERSION 3.21)

# The version comes from the repository's version.txt, which create-release.ps1 bumps on every run.
file(READ "${CMAKE_CURRENT_SOURCE_DIR}/../version.txt" COMPANION_VERSION)
string(STRIP "${COMPANION_VERSION}" COMPANION_VERSION)
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/../version.txt")

project(
    SkyrimCompanionExporter
    VERSION ${COMPANION_VERSION}
    DESCRIPTION "Streams Skyrim game state to the Skyrim Companion app"
    LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)

# CommonLibSSE-NG from the maintained fork (alandtse/CommonLibVR, ng branch). The Color-Glass vcpkg port
# stopped at 3.7.0 (May 2023) and predates the 1.6.1130+ and 1.7.x runtimes.
include(FetchContent)
set(BUILD_TESTS OFF CACHE BOOL "" FORCE)
FetchContent_Declare(CommonLibSSE
    GIT_REPOSITORY https://github.com/alandtse/CommonLibVR.git
    GIT_TAG v15.0.1
    GIT_SHALLOW ON)
FetchContent_MakeAvailable(CommonLibSSE)
# A source build doesn't define add_commonlibsse_plugin(); only the fork's prebuilt/installed package does.
include(${commonlibsse_SOURCE_DIR}/cmake/CommonLibSSE.cmake)
find_package(nlohmann_json CONFIG REQUIRED)

add_commonlibsse_plugin(${PROJECT_NAME}
    AUTHOR "Chad"
    SOURCES src/plugin.cpp src/snapshot.cpp src/events.cpp src/server.cpp src/net.cpp)

target_precompile_headers(${PROJECT_NAME} PRIVATE src/PCH.h)
target_compile_definitions(${PROJECT_NAME} PRIVATE COMPANION_VERSION="${PROJECT_VERSION}")
# File and product version shown in Explorer, the same version as the companion exes.
configure_file(src/version.rc.in ${CMAKE_CURRENT_BINARY_DIR}/version.rc @ONLY)
target_sources(${PROJECT_NAME} PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/version.rc)
# net.cpp includes WinSock2.h, which clashes with CommonLib's own Windows declarations.
set_source_files_properties(src/net.cpp PROPERTIES SKIP_PRECOMPILE_HEADERS ON)
target_link_libraries(${PROJECT_NAME} PRIVATE nlohmann_json::nlohmann_json ws2_32)

# Optional: set SKYRIM_DATA to copy the DLL straight into the game after each build, e.g.
# cmake --preset release -DSKYRIM_DATA="E:/Games/SteamRoot/Steam/steamapps/common/Skyrim Special Edition/Data"
if(DEFINED SKYRIM_DATA)
    add_custom_command(TARGET ${PROJECT_NAME} POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E make_directory "${SKYRIM_DATA}/SKSE/Plugins"
        COMMAND ${CMAKE_COMMAND} -E copy_if_different "$<TARGET_FILE:${PROJECT_NAME}>" "${SKYRIM_DATA}/SKSE/Plugins/")
endif()
