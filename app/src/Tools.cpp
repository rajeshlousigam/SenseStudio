#include "Tools.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>

namespace {

QStringList searchDirs()
{
    QStringList dirs{QCoreApplication::applicationDirPath()};
    dirs << QString::fromLocal8Bit(qgetenv("PATH")).split(QDir::listSeparator(), Qt::SkipEmptyParts);
#ifndef Q_OS_WIN
    dirs << QStringLiteral("/opt/homebrew/bin") << QStringLiteral("/usr/local/bin");
#endif
    return dirs;
}

QString findExecutable(const QString &name, const QString &devFallback = {})
{
#ifdef Q_OS_WIN
    const QString exe = name + QStringLiteral(".exe");
#else
    const QString &exe = name;
#endif
    QStringList dirs = searchDirs();
    // Bundled copy wins, then the dev build, then whatever is on PATH.
    if (!devFallback.isEmpty())
        dirs.insert(1, QFileInfo(devFallback).absolutePath());
    for (const QString &dir : std::as_const(dirs)) {
        const QFileInfo candidate(QDir(dir).filePath(exe));
        if (candidate.isFile() && candidate.isExecutable())
            return candidate.absoluteFilePath();
    }
    return {};
}

} // namespace

namespace Tools {

QString senseCli()
{
    static const QString path = findExecutable(QStringLiteral("sense-cli"), QStringLiteral(SENSE_DEV_CLI));
    return path;
}

QString ffmpeg()
{
    static const QString path = findExecutable(QStringLiteral("ffmpeg"));
    return path;
}

QString ffprobe()
{
    static const QString path = findExecutable(QStringLiteral("ffprobe"));
    return path;
}

QProcessEnvironment environment()
{
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    const QString ff = ffmpeg();
    if (!ff.isEmpty()) {
        const QString dir = QDir::toNativeSeparators(QFileInfo(ff).absolutePath());
        env.insert(QStringLiteral("PATH"), dir + QDir::listSeparator() + env.value(QStringLiteral("PATH")));
    }
    return env;
}

} // namespace Tools
