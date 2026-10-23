#include "EngineTask.h"

#include "Tools.h"

#include <QJsonDocument>
#include <QPointer>
#include <QProcess>
#include <QTimer>

namespace {
constexpr qsizetype kStderrTail = 4096;
}

EngineTask::EngineTask(QObject *parent)
    : QObject(parent)
{
}

EngineTask::~EngineTask()
{
    if (!m_proc)
        return;
    // Closing stdin is the sidecar's cancel signal; it stops its child tool and cleans up.
    m_proc->disconnect(this);
    m_proc->closeWriteChannel();
    if (!m_proc->waitForFinished(3000))
        m_proc->kill();
}

bool EngineTask::start(const QStringList &args)
{
    if (m_proc)
        return false;
    const QString cli = Tools::senseCli();
    if (cli.isEmpty()) {
        setMessage(tr("sense-cli was not found next to the app or on PATH"));
        emit finished(false, {});
        return false;
    }

    m_pending.clear();
    m_stderrTail.clear();
    m_lastResult.clear();
    m_anyFailure = false;
    m_cancelRequested = false;
    m_progress = 0;
    m_speedText.clear();
    m_index = 0;
    m_total = 0;
    setMessage({});

    m_proc = new QProcess(this);
    m_proc->setProcessEnvironment(Tools::environment());
    connect(m_proc, &QProcess::readyReadStandardOutput, this, [this] {
        m_pending.append(m_proc->readAllStandardOutput());
        qsizetype newline;
        while ((newline = m_pending.indexOf('\n')) >= 0) {
            const QByteArray line = m_pending.left(newline);
            m_pending.remove(0, newline + 1);
            handleLine(line);
        }
    });
    connect(m_proc, &QProcess::readyReadStandardError, this, [this] {
        m_stderrTail.append(m_proc->readAllStandardError());
        if (m_stderrTail.size() > kStderrTail)
            m_stderrTail = m_stderrTail.right(kStderrTail);
    });
    connect(m_proc, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        if (!m_pending.trimmed().isEmpty())
            handleLine(m_pending);
        QString failure;
        if (code != 0 && m_lastResult.isEmpty() && !m_cancelRequested)
            failure = QString::fromLocal8Bit(m_stderrTail).trimmed().section(QLatin1Char('\n'), -1);
        done(status == QProcess::CrashExit, failure);
    });
    connect(m_proc, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart)
            done(false, tr("Could not start sense-cli: %1").arg(m_proc->errorString()));
    });

    m_proc->start(cli, args);
    emit runningChanged();
    emit progressChanged();
    return true;
}

void EngineTask::cancel()
{
    if (!m_proc || m_cancelRequested)
        return;
    m_cancelRequested = true;
    m_proc->write("cancel\n");
    setMessage(tr("Cancelling…"));
    QTimer::singleShot(8000, this, [proc = QPointer<QProcess>(m_proc)] {
        if (proc && proc->state() != QProcess::NotRunning)
            proc->kill();
    });
}

void EngineTask::handleLine(const QByteArray &line)
{
    const QVariantMap ev = QJsonDocument::fromJson(line).toVariant().toMap();
    if (ev.isEmpty())
        return;
    const QString type = ev.value(QStringLiteral("event")).toString();
    if (type == QLatin1String("start")) {
        m_index = ev.value(QStringLiteral("index")).toInt();
        m_total = ev.value(QStringLiteral("total")).toInt();
        m_progress = 0;
        m_speedText.clear();
        emit progressChanged();
    } else if (type == QLatin1String("progress")) {
        m_progress = ev.value(QStringLiteral("fraction")).toDouble();
        const QVariant speed = ev.value(QStringLiteral("speed"));
        m_speedText = ev.contains(QStringLiteral("speed_text")) ? ev.value(QStringLiteral("speed_text")).toString()
                      : speed.isNull()                          ? QString()
                                                                : QStringLiteral("%1× realtime").arg(speed.toDouble(), 0, 'f', 1);
        emit progressChanged();
    } else if (type == QLatin1String("result") || type.isEmpty()) {
        m_lastResult = ev;
        const QString status = ev.value(QStringLiteral("status")).toString();
        if (status == QLatin1String("ERROR") || status == QLatin1String("CANCELLED"))
            m_anyFailure = true;
    }
    emit event(ev);
}

void EngineTask::done(bool crashed, const QString &failure)
{
    if (!m_proc)
        return;
    m_proc->deleteLater();
    m_proc = nullptr;
    const bool ok = !crashed && failure.isEmpty() && !m_anyFailure && !m_cancelRequested;
    if (crashed)
        setMessage(tr("Engine crashed"));
    else if (!failure.isEmpty())
        setMessage(failure);
    else if (m_cancelRequested)
        setMessage(tr("Cancelled"));
    else
        setMessage(m_lastResult.value(QStringLiteral("details")).toString());
    emit runningChanged();
    emit progressChanged();
    emit finished(ok, m_lastResult);
}

void EngineTask::setMessage(const QString &message)
{
    if (message == m_message)
        return;
    m_message = message;
    emit messageChanged();
}
