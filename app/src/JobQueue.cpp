#include "JobQueue.h"

#include "Tools.h"

#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QSettings>
#include <QStandardPaths>
#include <QTimer>
#include <QUrl>

namespace {

const QString kOutputDirKey = QStringLiteral("jobs/outputDir");
const QString kEncoderKey = QStringLiteral("jobs/encoder");
constexpr qsizetype kStderrTail = 4096;

} // namespace

JobQueue::JobQueue(QObject *parent)
    : QObject(parent)
{
    QSettings settings;
    const QString movies = QStandardPaths::writableLocation(QStandardPaths::MoviesLocation);
    m_outputDir = QDir::cleanPath(settings.value(kOutputDirKey, QDir(movies).filePath(QStringLiteral("SenseStudio"))).toString());
    m_encoder = settings.value(kEncoderKey, QStringLiteral("auto")).toString();
}

JobQueue::~JobQueue()
{
    if (!m_proc)
        return;
    // Closing stdin is the sidecar's cancel signal; it kills FFmpeg and removes the partial file.
    m_proc->disconnect(this);
    m_proc->closeWriteChannel();
    if (!m_proc->waitForFinished(3000))
        m_proc->kill();
}

void JobQueue::setLibrary(MediaLibrary *library)
{
    if (m_library == library)
        return;
    m_library = library;
    emit libraryChanged();
}

void JobQueue::setOutputDir(const QString &dir)
{
    // cleanPath drops the trailing slash folder pickers add, which broke the toolbar label.
    const QString clean = QDir::cleanPath(dir);
    if (dir.isEmpty() || clean == m_outputDir)
        return;
    m_outputDir = clean;
    QSettings().setValue(kOutputDirKey, clean);
    emit outputDirChanged();
}

void JobQueue::overrideOutputDir(const QString &dir)
{
    if (dir.isEmpty())
        return;
    m_outputDir = QDir::cleanPath(dir);
    emit outputDirChanged();
}

void JobQueue::setEncoder(const QString &encoder)
{
    if (encoder == m_encoder)
        return;
    m_encoder = encoder;
    QSettings().setValue(kEncoderKey, encoder);
    emit encoderChanged();
}

void JobQueue::start()
{
    if (m_proc || !m_library)
        return;
    const QString cli = Tools::senseCli();
    if (cli.isEmpty()) {
        setMessage(tr("sense-cli was not found next to the app or on PATH"));
        return;
    }
    m_paths = m_library->pathsToConvert();
    if (m_paths.isEmpty()) {
        setMessage(tr("Nothing to convert: every file is already StepOut Ready"));
        return;
    }
    if (!QDir().mkpath(m_outputDir)) {
        setMessage(tr("Cannot create output folder %1").arg(m_outputDir));
        return;
    }

    for (const QString &path : std::as_const(m_paths))
        m_library->setJobState(path, MediaLibrary::Queued, 0);
    m_library->setLocked(true);
    m_done = 0;
    m_currentName.clear();
    m_currentProgress = 0;
    m_speed = 0;
    m_pending.clear();
    m_stderrTail.clear();
    m_cancelRequested = false;
    setMessage({});

    m_proc = new QProcess(this);
    m_proc->setProcessEnvironment(Tools::environment());
    connect(m_proc, &QProcess::readyReadStandardOutput, this, [this] {
        m_pending.append(m_proc->readAllStandardOutput());
        qsizetype newline;
        while ((newline = m_pending.indexOf('\n')) >= 0) {
            const QByteArray line = m_pending.left(newline);
            m_pending.remove(0, newline + 1);
            handleEvent(line);
        }
    });
    connect(m_proc, &QProcess::readyReadStandardError, this, [this] {
        m_stderrTail.append(m_proc->readAllStandardError());
        if (m_stderrTail.size() > kStderrTail)
            m_stderrTail = m_stderrTail.right(kStderrTail);
    });
    connect(m_proc, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        QString failure;
        // Exit code 1 with per-file errors is reported per file; only flag a crash or a startup error.
        if (status == QProcess::CrashExit)
            failure = tr("Engine crashed");
        else if (code != 0 && m_done == 0 && !m_cancelRequested)
            failure = QString::fromLocal8Bit(m_stderrTail).trimmed().section(QLatin1Char('\n'), -1);
        finish(failure);
    });
    connect(m_proc, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart)
            finish(tr("Could not start sense-cli: %1").arg(m_proc->errorString()));
    });

    QStringList args{QStringLiteral("fix"), QStringLiteral("--events"),
                     QStringLiteral("--preset"), m_library->preset(),
                     QStringLiteral("--encoder"), m_encoder,
                     QStringLiteral("--output-dir"), m_outputDir};
    args += m_paths;
    m_proc->start(cli, args);
    emit runningChanged();
    emit progressChanged();
}

void JobQueue::cancel()
{
    if (!m_proc || m_cancelRequested)
        return;
    m_cancelRequested = true;
    m_proc->write("cancel\n");
    setMessage(tr("Cancelling…"));
    // The sidecar normally exits within a second; don't let a wedged FFmpeg hold the UI hostage.
    QTimer::singleShot(8000, this, [proc = QPointer<QProcess>(m_proc)] {
        if (proc && proc->state() != QProcess::NotRunning)
            proc->kill();
    });
}

void JobQueue::revealOutput() const
{
    QDesktopServices::openUrl(QUrl::fromLocalFile(m_outputDir));
}

void JobQueue::reveal(const QString &file) const
{
    const QFileInfo info(file);
    QDesktopServices::openUrl(QUrl::fromLocalFile(info.exists() ? info.absolutePath() : m_outputDir));
}

void JobQueue::handleEvent(const QByteArray &line)
{
    const QJsonObject ev = QJsonDocument::fromJson(line).object();
    const QString type = ev.value(QStringLiteral("event")).toString();
    const int index = ev.value(QStringLiteral("index")).toInt(-1);
    if (index < 0 || index >= m_paths.size() || !m_library)
        return;
    const QString &path = m_paths.at(index);

    if (type == QLatin1String("start")) {
        m_currentName = QFileInfo(path).fileName();
        m_currentProgress = 0;
        m_speed = 0;
        m_library->setJobState(path, MediaLibrary::Converting, 0);
    } else if (type == QLatin1String("progress")) {
        m_currentProgress = ev.value(QStringLiteral("fraction")).toDouble();
        m_speed = ev.value(QStringLiteral("speed")).toDouble();
        // Links that fail the check are downloaded first, then converted.
        const bool downloading = ev.value(QStringLiteral("stage")).toString() == QLatin1String("download");
        m_currentName = (downloading ? tr("Downloading ") : QString()) + QFileInfo(path).fileName();
        m_library->setJobState(path, MediaLibrary::Converting, m_currentProgress, downloading ? tr("Downloading…") : QString());
    } else if (type == QLatin1String("result")) {
        const QString status = ev.value(QStringLiteral("status")).toString();
        const QString details = ev.value(QStringLiteral("details")).toString();
        const QString output = ev.value(QStringLiteral("output_file")).toString();
        MediaLibrary::Status next = MediaLibrary::Failed;
        if (status.endsWith(QLatin1String("_VALIDATED")))
            next = MediaLibrary::Converted;
        else if (status == QLatin1String("READY"))
            next = MediaLibrary::Ready;
        else if (status == QLatin1String("CANCELLED"))
            next = MediaLibrary::Cancelled;
        m_library->setJobState(path, next, next == MediaLibrary::Converted ? 1.0 : 0.0, details, output);
        ++m_done;
    }
    emit progressChanged();
}

void JobQueue::finish(const QString &failure)
{
    if (!m_proc)
        return;
    m_proc->deleteLater();
    m_proc = nullptr;
    if (m_library) {
        // Files the engine never reached (cancel or crash) go back to a retryable state.
        for (const QString &path : std::as_const(m_paths)) {
            const auto status = m_library->statusOf(path);
            if (status == MediaLibrary::Queued || status == MediaLibrary::Converting)
                m_library->setJobState(path, MediaLibrary::Cancelled, 0, failure.isEmpty() ? tr("Not processed") : failure);
        }
        m_library->setLocked(false);
    }
    m_currentName.clear();
    if (!failure.isEmpty())
        setMessage(failure);
    else if (m_cancelRequested)
        setMessage(tr("Cancelled after %1 of %2").arg(m_done).arg(m_paths.size()));
    else
        setMessage(tr("Finished %1 file(s)").arg(m_done));
    emit runningChanged();
    emit progressChanged();
}

void JobQueue::setMessage(const QString &message)
{
    if (message == m_message)
        return;
    m_message = message;
    emit messageChanged();
}
