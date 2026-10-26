cmake_minimum_required(VERSION 3.21)
# Qt 6.8 and the bundled FFmpeg builds both require macOS 12+.
set(CMAKE_OSX_DEPLOYMENT_TARGET "12.0" CACHE STRING "Minimum macOS version")
project(SenseStudio VERSION 1.0.0 LANGUAGES CXX)
set(SENSE_VERSION_LABEL "1.0.0-beta.1" CACHE STRING "Version shown in the app and release names")

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 6.8 REQUIRED COMPONENTS Quick QuickControls2 Multimedia Svg)
qt_standard_project_setup(REQUIRES 6.8)

# The media engine is the Rust sense-cli sidecar; build it with the app so one command builds everything.
find_program(CARGO cargo REQUIRED)
set(ENGINE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/..)
set(SENSE_CLI_NAME sense-cli${CMAKE_EXECUTABLE_SUFFIX})
# Set RUST_TARGET (e.g. x86_64-apple-darwin) to cross-build the engine with the app.
set(RUST_TARGET "" CACHE STRING "Rust target triple for sense-cli; empty = host")
if(RUST_TARGET)
    set(SENSE_CLI_BIN ${ENGINE_DIR}/target/${RUST_TARGET}/release/${SENSE_CLI_NAME})
    set(CARGO_TARGET_ARGS --target ${RUST_TARGET})
else()
    set(SENSE_CLI_BIN ${ENGINE_DIR}/target/release/${SENSE_CLI_NAME})
    set(CARGO_TARGET_ARGS "")
endif()
add_custom_target(sense_cli ALL
    COMMAND ${CARGO} build --release -p sense-cli ${CARGO_TARGET_ARGS}
    WORKING_DIRECTORY ${ENGINE_DIR}
    BYPRODUCTS ${SENSE_CLI_BIN}
    COMMENT "Building sense-cli (Rust engine)"
    USES_TERMINAL)

set(APP_ICON_MAC ${CMAKE_CURRENT_SOURCE_DIR}/resources/SenseStudio.icns)
set_source_files_properties(${APP_ICON_MAC} PROPERTIES MACOSX_PACKAGE_LOCATION Resources)

qt_add_executable(SenseStudio
    src/main.cpp
    ${APP_ICON_MAC}
    src/Tools.h src/Tools.cpp
    src/MediaLibrary.h src/MediaLibrary.cpp
    src/JobQueue.h src/JobQueue.cpp
    src/ThumbnailProvider.h src/ThumbnailProvider.cpp
    src/Util.h src/Util.cpp
    src/EngineTask.h src/EngineTask.cpp
    src/SequenceModel.h src/SequenceModel.cpp
)
add_dependencies(SenseStudio sense_cli)
if(WIN32)
    enable_language(RC)
    target_sources(SenseStudio PRIVATE resources/SenseStudio.rc)
endif()

set_source_files_properties(qml/Theme.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

qt_add_qml_module(SenseStudio
    URI SenseStudio
    VERSION 1.0
    QML_FILES
        qml/Main.qml
        qml/Theme.qml
        qml/Toolbar.qml
        qml/LibraryPanel.qml
        qml/MediaCard.qml
        qml/PlayerPanel.qml
        qml/DetailsPanel.qml
        qml/StatusBar.qml
        qml/SButton.qml
        qml/IconButton.qml
        qml/SComboBox.qml
        qml/StatusBadge.qml
        qml/SCheckBox.qml
        qml/NavRail.qml
        qml/Timeline.qml
        qml/EditView.qml
        qml/DownloadsView.qml
    RESOURCES
        qml/icons/close.svg
        qml/icons/down.svg
        qml/icons/download.svg
        qml/icons/export.svg
        qml/icons/film.svg
        qml/icons/folder.svg
        qml/icons/library.svg
        qml/icons/mute.svg
        qml/icons/pause.svg
        qml/icons/play.svg
        qml/icons/plus.svg
        qml/icons/retry.svg
        qml/icons/scissors.svg
        qml/icons/skip-back.svg
        qml/icons/skip-forward.svg
        qml/icons/step-back.svg
        qml/icons/step-forward.svg
        qml/icons/trash.svg
        qml/icons/up.svg
        qml/icons/volume.svg
)

# Generated QML type registrations include the headers by bare name.
target_include_directories(SenseStudio PRIVATE src)
target_compile_definitions(SenseStudio PRIVATE SENSE_DEV_CLI="${SENSE_CLI_BIN}" SENSE_VERSION="${SENSE_VERSION_LABEL}")
target_link_libraries(SenseStudio PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Multimedia Qt6::Svg)

set_target_properties(SenseStudio PROPERTIES
    MACOSX_BUNDLE TRUE
    MACOSX_BUNDLE_GUI_IDENTIFIER ai.stepout.sensestudio
    MACOSX_BUNDLE_BUNDLE_VERSION ${PROJECT_VERSION}
    MACOSX_BUNDLE_SHORT_VERSION_STRING ${SENSE_VERSION_LABEL}
    MACOSX_BUNDLE_BUNDLE_NAME SenseStudio
    MACOSX_BUNDLE_ICON_FILE SenseStudio.icns
    WIN32_EXECUTABLE TRUE)

# Ship the sidecar next to the app executable (Contents/MacOS on macOS), where Tools looks first.
add_custom_command(TARGET SenseStudio POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_if_different ${SENSE_CLI_BIN} $<TARGET_FILE_DIR:SenseStudio>/${SENSE_CLI_NAME})
