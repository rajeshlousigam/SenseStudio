#pragma once

#include <QObject>
#include <QPointer>
#include <QStringList>
#include <QUrl>
#include <QtQmlIntegration>

#include "MediaLibrary.h"

class QProcess;

// Runs `sense-cli fix --events` over the files that need it. Jobs run one at a
// time on purpose: on 2-4 core machines a second encode only makes both slower.
class JobQueue : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(MediaLibrary *library READ library WRITE setLibrary NOTIFY libraryChanged)
    Q_PROPERTY(bool running READ running NOTIFY runningChanged)
    Q_PROPERTY(QString outputDir READ outputDir WRITE setOutputDir NOTIFY outputDirChanged)
    Q_PROPERTY(QString encoder READ encoder WRITE setEncoder NOTIFY encoderChanged)
    Q_PROPERTY(int total READ total NOTIFY progressChanged)
    Q_PROPERTY(int done READ done NOTIFY progressChanged)
    Q_PROPERTY(QString currentName READ currentName NOTIFY progressChanged)
    Q_PROPERTY(double currentProgress READ currentProgress NOTIFY progressChanged)
    Q_PROPERTY(double speed READ speed NOTIFY progressChanged)
    Q_PROPERTY(QString message READ message NOTIFY messageChanged)

public:
    explicit JobQueue(QObject *parent = nullptr);
    ~JobQueue() override;

    MediaLibrary *library() const { return m_library; }
    void setLibrary(MediaLibrary *library);
    bool running() const { return m_proc != nullptr; }
    QString outputDir() const { return m_outputDir; }
    void setOutputDir(const QString &dir);
    // Session-only output folder (command line); not saved to settings.
    Q_INVOKABLE void overrideOutputDir(const QString &dir);
    QString encoder() const { return m_encoder; }
    void setEncoder(const QString &encoder);
    int total() const { return int(m_paths.size()); }
    int done() const { return m_done; }
    QString currentName() const { return m_currentName; }
    double currentProgress() const { return m_currentProgress; }
    double speed() const { return m_speed; }
    QString message() const { return m_message; }

    Q_INVOKABLE void start();
    Q_INVOKABLE void cancel();
    Q_INVOKABLE void revealOutput() const;
    Q_INVOKABLE void reveal(const QString &file) const;
    Q_INVOKABLE void setOutputDirUrl(const QUrl &url) { setOutputDir(url.toLocalFile()); }

signals:
    void libraryChanged();
    void runningChanged();
    void outputDirChanged();
    void encoderChanged();
    void progressChanged();
    void messageChanged();

private:
    void handleEvent(const QByteArray &line);
    void finish(const QString &failure);
    void setMessage(const QString &message);

    QPointer<MediaLibrary> m_library;
    QProcess *m_proc = nullptr;
    QByteArray m_pending;
    QByteArray m_stderrTail;
    QStringList m_paths;
    int m_done = 0;
    QString m_currentName;
    double m_currentProgress = 0;
    double m_speed = 0;
    QString m_outputDir;
    QString m_encoder;
    QString m_message;
    bool m_cancelRequested = false;
};
