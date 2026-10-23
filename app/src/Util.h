#pragma once

#include <QObject>
#include <QTemporaryDir>
#include <QUrl>
#include <QVariant>
#include <QtQmlIntegration>

#include <memory>

// Small file-system helpers for QML, which has no direct file access.
class Util : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON

public:
    explicit Util(QObject *parent = nullptr);

    // Session scratch directory, removed when the app exits.
    Q_INVOKABLE QString tempDir();
    Q_INVOKABLE QString makeTempSubdir(const QString &prefix);
    Q_INVOKABLE QString proxyPathFor(const QString &path) const;
    Q_INVOKABLE bool exists(const QString &path) const;
    Q_INVOKABLE QUrl fileUrl(const QString &path) const;
    Q_INVOKABLE QString localPath(const QUrl &url) const;
    Q_INVOKABLE QString fileName(const QString &path) const;
    Q_INVOKABLE QString baseName(const QString &path) const;
    Q_INVOKABLE QString joinPath(const QString &dir, const QString &name) const;
    // `<dir>/<base>.<ext>`, or `<base> 2.<ext>` etc. if taken; creates dir.
    Q_INVOKABLE QString uniquePath(const QString &dir, const QString &base, const QString &ext) const;
    Q_INVOKABLE bool writeJson(const QString &path, const QVariant &value) const;
    Q_INVOKABLE QVariant readJson(const QString &path) const;
    Q_INVOKABLE void reveal(const QString &path) const;
    Q_INVOKABLE QString timestamp() const;

private:
    std::unique_ptr<QTemporaryDir> m_temp;
};
