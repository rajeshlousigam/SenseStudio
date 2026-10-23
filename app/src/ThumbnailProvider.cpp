#include "ThumbnailProvider.h"

#include "Tools.h"

#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QFileInfo>
#include <QImage>
#include <QProcess>
#include <QRunnable>
#include <QStandardPaths>

#include <algorithm>

namespace {

constexpr int kThumbWidth = 320;
constexpr int kTimeoutMs = 20000;
constexpr int kStripHeight = 72;
constexpr int kStripTimeoutMs = 120000;

QString cacheDir()
{
    static const QString dir = [] {
        const QString d = QDir(QStandardPaths::writableLocation(QStandardPaths::CacheLocation)).filePath(QStringLiteral("thumbs"));
        QDir().mkpath(d);
        return d;
    }();
    return dir;
}

bool extractFrame(const QString &ffmpeg, const QString &input, const QString &output, bool seek)
{
    QStringList args{QStringLiteral("-hide_banner"), QStringLiteral("-nostdin"), QStringLiteral("-v"), QStringLiteral("error"), QStringLiteral("-y")};
    if (seek) // input seeking is keyframe-fast; 1s skips black lead-in frames
        args << QStringLiteral("-ss") << QStringLiteral("1");
    args << QStringLiteral("-i") << input << QStringLiteral("-frames:v") << QStringLiteral("1")
         << QStringLiteral("-vf") << QStringLiteral("scale=%1:-2:flags=bilinear").arg(kThumbWidth)
         << QStringLiteral("-q:v") << QStringLiteral("5") << output;
    QProcess proc;
    proc.setProcessEnvironment(Tools::environment());
    proc.start(ffmpeg, args);
    if (!proc.waitForFinished(kTimeoutMs)) {
        proc.kill();
        proc.waitForFinished(1000);
        return false;
    }
    return proc.exitStatus() == QProcess::NormalExit && proc.exitCode() == 0 && QFileInfo(output).size() > 0;
}

double probeDuration(const QString &input)
{
    const QString ffprobe = Tools::ffprobe();
    if (ffprobe.isEmpty())
        return 0;
    QProcess proc;
    proc.setProcessEnvironment(Tools::environment());
    proc.start(ffprobe, {QStringLiteral("-v"), QStringLiteral("error"), QStringLiteral("-show_entries"),
                         QStringLiteral("format=duration"), QStringLiteral("-of"), QStringLiteral("csv=p=0"), input});
    if (!proc.waitForFinished(kTimeoutMs))
        return 0;
    return QString::fromLatin1(proc.readAllStandardOutput()).trimmed().toDouble();
}

// One image of `count` evenly spaced frames. Long files decode keyframes only, which is
// what makes a 90-minute match strip take seconds instead of minutes.
bool extractStrip(const QString &ffmpeg, const QString &input, const QString &output, int count)
{
    const double duration = probeDuration(input);
    if (duration <= 0)
        return false;
    QStringList args{QStringLiteral("-hide_banner"), QStringLiteral("-nostdin"), QStringLiteral("-v"), QStringLiteral("error"), QStringLiteral("-y")};
    if (duration > 60)
        args << QStringLiteral("-skip_frame") << QStringLiteral("nokey");
    args << QStringLiteral("-i") << input << QStringLiteral("-an")
         << QStringLiteral("-vf")
         << QStringLiteral("fps=%1,scale=-2:%2:flags=bilinear,tile=%3x1").arg(count / duration, 0, 'f', 6).arg(kStripHeight).arg(count)
         << QStringLiteral("-frames:v") << QStringLiteral("1") << QStringLiteral("-q:v") << QStringLiteral("6") << output;
    QProcess proc;
    proc.setProcessEnvironment(Tools::environment());
    proc.start(ffmpeg, args);
    if (!proc.waitForFinished(kStripTimeoutMs)) {
        proc.kill();
        proc.waitForFinished(1000);
        return false;
    }
    return proc.exitStatus() == QProcess::NormalExit && proc.exitCode() == 0 && QFileInfo(output).size() > 0;
}

class ThumbnailResponse : public QQuickImageResponse, public QRunnable
{
public:
    ThumbnailResponse(QString path, QSize requested, int stripCount)
        : m_path(std::move(path)), m_requested(requested), m_stripCount(stripCount)
    {
        setAutoDelete(false);
    }

    QQuickTextureFactory *textureFactory() const override
    {
        return QQuickTextureFactory::textureFactoryForImage(m_image);
    }

    QString errorString() const override { return m_error; }

    void run() override
    {
        load();
        emit finished();
    }

private:
    void load()
    {
        // Links: FFmpeg range-seeks to the poster frame, so only a little of the file is fetched.
        const bool link = m_path.startsWith(QLatin1String("http://")) || m_path.startsWith(QLatin1String("https://"));
        const QFileInfo info(m_path);
        if (!link && !info.isFile()) {
            m_error = QStringLiteral("File not found");
            return;
        }
        const QString identity = link ? m_path
            : m_path + QLatin1Char('|') + QString::number(info.size()) + QLatin1Char('|')
                + QString::number(info.lastModified().toMSecsSinceEpoch());
        const QByteArray key = QCryptographicHash::hash(
            (identity + QLatin1Char('|') + QString::number(m_stripCount)).toUtf8(), QCryptographicHash::Sha1).toHex();
        const QString cached = QDir(cacheDir()).filePath(QString::fromLatin1(key) + QStringLiteral(".jpg"));

        if (!QFileInfo::exists(cached)) {
            const QString ffmpeg = Tools::ffmpeg();
            if (ffmpeg.isEmpty()) {
                m_error = QStringLiteral("ffmpeg not found");
                return;
            }
            // Write then rename so a half-written file is never served from the cache.
            const QString tmp = cached + QStringLiteral(".part.jpg");
            const bool ok = m_stripCount > 0
                ? extractStrip(ffmpeg, m_path, tmp, m_stripCount)
                : extractFrame(ffmpeg, m_path, tmp, true) || extractFrame(ffmpeg, m_path, tmp, false);
            if (!ok) {
                QFile::remove(tmp);
                m_error = QStringLiteral("Could not extract a frame");
                return;
            }
            QFile::remove(cached);
            QFile::rename(tmp, cached);
        }

        QImage image(cached);
        if (image.isNull()) {
            m_error = QStringLiteral("Corrupt thumbnail");
            QFile::remove(cached);
            return;
        }
        if (m_stripCount == 0 && m_requested.isValid() && m_requested.width() > 0 && m_requested.width() < image.width())
            image = image.scaledToWidth(m_requested.width(), Qt::SmoothTransformation);
        m_image = image;
    }

    QString m_path;
    QSize m_requested;
    int m_stripCount = 0;
    QImage m_image;
    QString m_error;
};

} // namespace

ThumbnailProvider::ThumbnailProvider()
{
    // Each worker spawns an FFmpeg decode; two keeps low-end machines responsive.
    m_pool.setMaxThreadCount(2);
}

QQuickImageResponse *ThumbnailProvider::requestImageResponse(const QString &id, const QSize &requestedSize)
{
    int stripCount = 0;
    QString encoded = id;
    if (id.startsWith(QLatin1String("strip/"))) {
        const QStringList parts = id.split(QLatin1Char('/'));
        if (parts.size() == 3) {
            stripCount = std::clamp(parts.at(1).toInt(), 1, 200);
            encoded = parts.at(2);
        }
    }
    const QString path = QString::fromUtf8(
        QByteArray::fromBase64(encoded.toLatin1(), QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals));
    auto *response = new ThumbnailResponse(path, requestedSize, stripCount);
    m_pool.start(response);
    return response;
}
