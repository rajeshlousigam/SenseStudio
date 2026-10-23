#pragma once

#include <QAbstractListModel>
#include <QHash>
#include <QStringList>
#include <QUrl>
#include <QVariantList>
#include <QtQmlIntegration>

class QProcess;

// Imported files with their StepOut Ready status. Probing runs in sense-cli
// sidecar processes in small batches so the UI thread only parses JSON lines.
class MediaLibrary : public QAbstractListModel
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(int checkedCount READ checkedCount NOTIFY checkedCountChanged)
    Q_PROPERTY(int fixableCount READ fixableCount NOTIFY revisionChanged)
    Q_PROPERTY(QString preset READ preset WRITE setPreset NOTIFY presetChanged)
    Q_PROPERTY(bool probing READ probing NOTIFY probingChanged)
    Q_PROPERTY(bool locked READ locked WRITE setLocked NOTIFY lockedChanged)
    // Bumped on every item change so QML bindings over get() re-evaluate.
    Q_PROPERTY(int revision READ revision NOTIFY revisionChanged)

public:
    enum Status {
        Probing,
        Ready,
        NeedsRemux,
        NeedsAudio,
        NeedsTranscode,
        Queued,
        Converting,
        Converted,
        Failed,
        Cancelled,
    };
    Q_ENUM(Status)

    enum Role {
        PathRole = Qt::UserRole + 1,
        NameRole,
        FolderRole,
        SizeRole,
        DurationRole,
        ResolutionRole,
        CodecRole,
        FpsRole,
        StatusRole,
        StatusTextRole,
        ViolationsRole,
        DetailsRole,
        ProgressRole,
        OutputRole,
        ThumbRole,
        CheckedRole,
        FileUrlRole,
        OutputUrlRole,
        RemoteRole,
    };

    explicit MediaLibrary(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    Q_INVOKABLE void addUrls(const QList<QUrl> &urls);
    // Video links (S3, CloudFront, any http(s) file URL), one per line or whitespace-separated.
    // Returns how many new links were added. They are checked without downloading.
    Q_INVOKABLE int addLinks(const QString &text);
    Q_INVOKABLE void removeAt(int row);
    Q_INVOKABLE void clear();
    Q_INVOKABLE void setChecked(int row, bool checked);
    Q_INVOKABLE void setAllChecked(bool checked);
    Q_INVOKABLE QVariantMap get(int row) const;

    int checkedCount() const;
    int fixableCount() const;
    QString preset() const { return m_preset; }
    void setPreset(const QString &preset);
    bool probing() const { return m_activeProbes > 0 || !m_probeQueue.isEmpty(); }
    bool locked() const { return m_locked; }
    void setLocked(bool locked);
    int revision() const { return m_revision; }

    // Used by JobQueue.
    QStringList pathsToConvert() const;
    Status statusOf(const QString &path) const;
    void setJobState(const QString &path, Status status, double progress,
                     const QString &details = {}, const QString &output = {});

signals:
    void countChanged();
    void checkedCountChanged();
    void presetChanged();
    void probingChanged();
    void lockedChanged();
    void revisionChanged();

private:
    struct Item {
        QString path;
        QString name;
        QString folder;
        qint64 size = 0;
        double duration = 0;
        int width = 0;
        int height = 0;
        QString codec;
        double fps = 0;
        Status status = Probing;
        QString details;
        QVariantList violations;
        double progress = 0;
        QString output;
        bool checked = false;
    };

    int rowForPath(const QString &path) const;
    void touch(int row);
    void enqueueProbe(const QStringList &paths);
    void pumpProbes();
    void handleProbeLine(const QByteArray &line);
    void failProbing(const QStringList &paths, const QString &reason);
    void appendItems(const QStringList &paths);
    static bool isLink(const QString &path);

    QList<Item> m_items;
    QHash<QString, int> m_rowByPath;
    QStringList m_probeQueue;
    int m_activeProbes = 0;
    int m_generation = 0;
    int m_revision = 0;
    QString m_preset = QStringLiteral("1080");
    bool m_locked = false;
};
