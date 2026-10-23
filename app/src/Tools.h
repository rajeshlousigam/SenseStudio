#pragma once

#include <QProcessEnvironment>
#include <QString>

// Locates the sidecar binaries. Apps started from Finder/Explorer do not
// inherit a shell PATH, so the app directory and common install prefixes
// are searched explicitly.
namespace Tools {

QString senseCli();
QString ffmpeg();
QString ffprobe();
// Environment for child processes: PATH starts with the directory holding ffmpeg,
// so sense-cli resolves the same FFmpeg the app uses.
QProcessEnvironment environment();

} // namespace Tools
