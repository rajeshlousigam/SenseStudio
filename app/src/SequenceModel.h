#pragma once

#include <QAbstractListModel>
#include <QtQmlIntegration>

// The Edit list: ordered clips with in/out points (source seconds).
// Exported as one merged StepOut Ready file.
class SequenceModel : public QAbstractListModel
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
    Q_PROPERTY(double totalDuration READ totalDuration NOTIFY revisionChanged)
    Q_PROPERTY(int revision READ revision NOTIFY revisionChanged)

public:
    enum Role {
        PathRole = Qt::UserRole + 1,
        NameRole,
        FileUrlRole,
        ThumbRole,
        DurationRole,
        FpsRole,
        InRole,
        OutRole,
        ClipDurationRole,
    };

    explicit SequenceModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    // `item` is a MediaLibrary.get() map (path, name, duration, fps).
    Q_INVOKABLE int addFromLibrary(const QVariantMap &item);
    Q_INVOKABLE void removeAt(int row);
    Q_INVOKABLE void move(int from, int to);
    Q_INVOKABLE void clear();
    Q_INVOKABLE QVariantMap get(int row) const;
    Q_INVOKABLE void setInPoint(int row, double seconds);
    Q_INVOKABLE void setOutPoint(int row, double seconds);
    // Render spec segments: [{ path, start, end }] in list order.
    Q_INVOKABLE QVariantList segments() const;

    double totalDuration() const;
    int revision() const { return m_revision; }

signals:
    void countChanged();
    void revisionChanged();

private:
    struct Clip {
        QString path;
        QString name;
        double duration = 0;
        double fps = 30;
        double in = 0;
        double out = 0;
    };

    void changed(int row);

    QList<Clip> m_clips;
    int m_revision = 0;
};
