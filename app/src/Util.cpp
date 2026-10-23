#include "Util.h"

#include <QCryptographicHash>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QSaveFile>
#include <QStandardPaths>

Util::Util(QObject *parent)
    : QObject(parent)
{
}

QString Util::tempDir()
{
    if (!m_temp) {
        m_temp = std::make_unique<QTemporaryDir>(QDir::tempPath() + QStringLiteral("/sensestudio-XXXXXX"));
        if (!m_temp->isValid())
            return QDir::tempPath();
    }
    return m_temp->path();
}

QString Util::makeTempSubdir(const QString &prefix)
{
    const QString dir = QDir(tempDir()).filePath(prefix + QLatin1Char('-') + QString::number(QDateTime::currentMSecsSinceEpoch()));
    QDir().mkpath(dir);
    return dir;
}

QString Util::proxyPathFor(const QString &path) const
{
    const QFileInfo info(path);
    const QByteArray key = QCryptographicHash::hash(
        // "intra" versions the key: proxies from the older short-GOP format are not reused.
        (path + QLatin1Char('|') + QString::number(info.size()) + QLatin1Char('|')
         + QString::number(info.lastModified().toMSecsSinceEpoch()) + QStringLiteral("|intra")).toUtf8(),
        QCryptographicHash::Sha1).toHex();
    const QString dir = QDir(QStandardPaths::writableLocation(QStandardPaths::CacheLocation)).filePath(QStringLiteral("proxies"));
    QDir().mkpath(dir);
    return QDir(dir).filePath(QString::fromLatin1(key) + QStringLiteral(".mp4"));
}

bool Util::exists(const QString &path) const
{
    return !path.isEmpty() && QFileInfo::exists(path);
}

QUrl Util::fileUrl(const QString &path) const
{
    return path.isEmpty() ? QUrl() : QUrl::fromLocalFile(path);
}

QString Util::localPath(const QUrl &url) const
{
    return url.toLocalFile();
}

QString Util::fileName(const QString &path) const
{
    return QFileInfo(path).fileName();
}

QString Util::baseName(const QString &path) const
{
    return QFileInfo(path).completeBaseName();
}

QString Util::joinPath(const QString &dir, const QString &name) const
{
    return QDir(dir).filePath(name);
}

QString Util::uniquePath(const QString &dir, const QString &base, const QString &ext) const
{
    QDir().mkpath(dir);
    QString candidate = QDir(dir).filePath(base + QLatin1Char('.') + ext);
    for (int n = 2; QFileInfo::exists(candidate); ++n)
        candidate = QDir(dir).filePath(QStringLiteral("%1 %2.%3").arg(base).arg(n).arg(ext));
    return candidate;
}

bool Util::writeJson(const QString &path, const QVariant &value) const
{
    // QSaveFile: a crash mid-write never leaves a truncated project file.
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly))
        return false;
    file.write(QJsonDocument::fromVariant(value).toJson(QJsonDocument::Indented));
    return file.commit();
}

QVariant Util::readJson(const QString &path) const
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
        return {};
    QJsonParseError error{};
    const QJsonDocument doc = QJsonDocument::fromJson(file.readAll(), &error);
    return error.error == QJsonParseError::NoError ? doc.toVariant() : QVariant();
}

void Util::reveal(const QString &path) const
{
    const QFileInfo info(path);
    QDesktopServices::openUrl(QUrl::fromLocalFile(info.isDir() ? path : info.absolutePath()));
}

QString Util::timestamp() const
{
    return QDateTime::currentDateTime().toString(QStringLiteral("yyyy-MM-dd HH.mm"));
}
