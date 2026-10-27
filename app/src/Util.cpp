#include "Util.h"

#include <QCryptographicHash>
#include <QDateTime>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QSaveFile>
#include <QProcess>
#include <QSettings>
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

namespace {

struct Browser {
    const char *id;
    const char *label;
    const char *macApp;     // bundle name in /Applications
    const char *windowsExe; // registered under "App Paths"
};

// Order matters: most common first. Safari exists only on macOS.
constexpr Browser kBrowsers[] = {
    {"chrome", "Chrome", "Google Chrome", "chrome.exe"},
    {"safari", "Safari", "Safari", nullptr},
    {"edge", "Edge", "Microsoft Edge", "msedge.exe"},
    {"firefox", "Firefox", "Firefox", "firefox.exe"},
    {"brave", "Brave", "Brave Browser", "brave.exe"},
};

const Browser *findBrowser(const QString &id)
{
    for (const Browser &b : kBrowsers) {
        if (id == QLatin1String(b.id))
            return &b;
    }
    return nullptr;
}

#if defined(Q_OS_WIN)
// Full path of the browser exe from its "App Paths" registration (machine-wide or per-user install).
QString windowsBrowserPath(const Browser &b)
{
    if (!b.windowsExe)
        return {};
    const QString key = QStringLiteral("\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\") + QString::fromLatin1(b.windowsExe);
    for (const QString &root : {QStringLiteral("HKEY_CURRENT_USER"), QStringLiteral("HKEY_LOCAL_MACHINE")}) {
        QString path = QSettings(root + key, QSettings::NativeFormat).value(QStringLiteral(".")).toString().trimmed();
        if (path.size() > 1 && path.startsWith(QLatin1Char('"')) && path.endsWith(QLatin1Char('"')))
            path = path.mid(1, path.size() - 2);
        if (QFileInfo(path).isFile())
            return path;
    }
    return {};
}
#endif

bool isInstalled(const Browser &b)
{
#if defined(Q_OS_MACOS)
    const QString app = QString::fromLatin1(b.macApp) + QStringLiteral(".app");
    return QFileInfo::exists(QStringLiteral("/Applications/") + app)
        || QFileInfo::exists(QDir::homePath() + QStringLiteral("/Applications/") + app)
        || (QLatin1String(b.id) == QLatin1String("safari") && QFileInfo::exists(QStringLiteral("/System/Volumes/Preboot/Cryptexes/App/System/Applications/Safari.app")));
#elif defined(Q_OS_WIN)
    return !windowsBrowserPath(b).isEmpty();
#else
    Q_UNUSED(b)
    return false;
#endif
}

} // namespace

QVariantList Util::installedBrowsers() const
{
    QVariantList list;
    for (const Browser &b : kBrowsers) {
        if (isInstalled(b))
            list.append(QVariantMap{{QStringLiteral("id"), QString::fromLatin1(b.id)}, {QStringLiteral("label"), QString::fromLatin1(b.label)}});
    }
    return list;
}

bool Util::openInBrowser(const QString &browser, const QUrl &url) const
{
    const Browser *b = findBrowser(browser);
    bool started = false;
#if defined(Q_OS_MACOS)
    if (b)
        started = QProcess::startDetached(QStringLiteral("/usr/bin/open"), {QStringLiteral("-a"), QString::fromLatin1(b->macApp), url.toString()});
#elif defined(Q_OS_WIN)
    // Launch the exe directly with the URL as one argument. Going through `cmd /c start` broke
    // sign-in links: cmd treats `&` as a command separator and `%xx` as variables.
    if (b) {
        const QString exe = windowsBrowserPath(*b);
        if (!exe.isEmpty())
            started = QProcess::startDetached(exe, {url.toString(QUrl::FullyEncoded)});
    }
#endif
    return started || QDesktopServices::openUrl(url);
}

QString Util::timestamp() const
{
    return QDateTime::currentDateTime().toString(QStringLiteral("yyyy-MM-dd HH.mm"));
}
