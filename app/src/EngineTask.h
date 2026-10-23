#pragma once

#include <QObject>
#include <QStringList>
#include <QVariantMap>
#include <QtQmlIntegration>

class QProcess;

// One sense-cli job (render, proxy, download) speaking the NDJSON
// event protocol. QML reacts to `event` and `finished`; cancel is via stdin.
class EngineTask : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(bool running READ running NOTIFY runningChanged)
    Q_PROPERTY(double progress READ progress NOTIFY progressChanged)
    Q_PROPERTY(QString speedText READ speedText NOTIFY progressChanged)
    Q_PROPERTY(int index READ index NOTIFY progressChanged)
    Q_PROPERTY(int total READ total NOTIFY progressChanged)
    Q_PROPERTY(QString message READ message NOTIFY messageChanged)

public:
    explicit EngineTask(QObject *parent = nullptr);
    ~EngineTask() override;

    bool running() const { return m_proc != nullptr; }
    double progress() const { return m_progress; }
    QString speedText() const { return m_speedText; }
    int index() const { return m_index; }
    int total() const { return m_total; }
    QString message() const { return m_message; }

    // `args` are sense-cli arguments; each NDJSON line is delivered via `event`.
    Q_INVOKABLE bool start(const QStringList &args);
    Q_INVOKABLE void cancel();

signals:
    void runningChanged();
    void progressChanged();
    void messageChanged();
    void event(const QVariantMap &event);
    void finished(bool ok, const QVariantMap &lastResult);

private:
    void handleLine(const QByteArray &line);
    void done(bool crashed, const QString &failure);
    void setMessage(const QString &message);

    QProcess *m_proc = nullptr;
    QByteArray m_pending;
    QByteArray m_stderrTail;
    QVariantMap m_lastResult;
    bool m_anyFailure = false;
    bool m_cancelRequested = false;
    double m_progress = 0;
    QString m_speedText;
    int m_index = 0;
    int m_total = 0;
    QString m_message;
};
