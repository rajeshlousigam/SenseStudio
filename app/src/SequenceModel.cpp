#include "SequenceModel.h"

#include <QFileInfo>
#include <QUrl>

#include <algorithm>

namespace {
constexpr double kMinClip = 0.04; // one frame at 25 fps
}

SequenceModel::SequenceModel(QObject *parent)
    : QAbstractListModel(parent)
{
}

int SequenceModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : int(m_clips.size());
}

QVariant SequenceModel::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_clips.size())
        return {};
    const Clip &c = m_clips.at(index.row());
    switch (role) {
    case PathRole: return c.path;
    case NameRole: return c.name;
    case FileUrlRole: return QUrl::fromLocalFile(c.path);
    case ThumbRole:
        return QStringLiteral("image://thumbs/")
            + QString::fromLatin1(c.path.toUtf8().toBase64(QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals));
    case DurationRole: return c.duration;
    case FpsRole: return c.fps;
    case InRole: return c.in;
    case OutRole: return c.out;
    case ClipDurationRole: return c.out - c.in;
    default: return {};
    }
}

QHash<int, QByteArray> SequenceModel::roleNames() const
{
    return {
        {PathRole, "path"}, {NameRole, "name"}, {FileUrlRole, "fileUrl"}, {ThumbRole, "thumb"},
        {DurationRole, "duration"}, {FpsRole, "fps"}, {InRole, "inPoint"}, {OutRole, "outPoint"},
        {ClipDurationRole, "clipDuration"},
    };
}

int SequenceModel::addFromLibrary(const QVariantMap &item)
{
    const QString path = item.value(QStringLiteral("path")).toString();
    const double duration = item.value(QStringLiteral("duration")).toDouble();
    // Local files only: the export probes and decodes each clip from disk.
    if (path.isEmpty() || duration <= kMinClip || !QFileInfo(path).isFile())
        return -1;
    Clip clip;
    clip.path = path;
    clip.name = item.value(QStringLiteral("name")).toString();
    if (clip.name.isEmpty())
        clip.name = QFileInfo(path).fileName();
    clip.duration = duration;
    const double fps = item.value(QStringLiteral("fps")).toDouble();
    clip.fps = fps > 0 ? fps : 30;
    clip.out = duration;

    const int row = int(m_clips.size());
    beginInsertRows({}, row, row);
    m_clips.append(std::move(clip));
    endInsertRows();
    emit countChanged();
    ++m_revision;
    emit revisionChanged();
    return row;
}

void SequenceModel::removeAt(int row)
{
    if (row < 0 || row >= m_clips.size())
        return;
    beginRemoveRows({}, row, row);
    m_clips.removeAt(row);
    endRemoveRows();
    emit countChanged();
    ++m_revision;
    emit revisionChanged();
}

void SequenceModel::move(int from, int to)
{
    if (from < 0 || from >= m_clips.size() || to < 0 || to >= m_clips.size() || from == to)
        return;
    // beginMoveRows wants the destination as an insertion index in the pre-move list.
    beginMoveRows({}, from, from, {}, to > from ? to + 1 : to);
    m_clips.move(from, to);
    endMoveRows();
    ++m_revision;
    emit revisionChanged();
}

void SequenceModel::clear()
{
    if (m_clips.isEmpty())
        return;
    beginResetModel();
    m_clips.clear();
    endResetModel();
    emit countChanged();
    ++m_revision;
    emit revisionChanged();
}

QVariantMap SequenceModel::get(int row) const
{
    if (row < 0 || row >= m_clips.size())
        return {};
    QVariantMap map;
    const QModelIndex idx = index(row);
    const auto names = roleNames();
    for (auto it = names.cbegin(); it != names.cend(); ++it)
        map.insert(QString::fromLatin1(it.value()), data(idx, it.key()));
    map.insert(QStringLiteral("output"), QString()); // PlayerPanel compatibility
    return map;
}

void SequenceModel::setInPoint(int row, double seconds)
{
    if (row < 0 || row >= m_clips.size())
        return;
    Clip &c = m_clips[row];
    c.in = std::clamp(seconds, 0.0, std::max(0.0, c.out - kMinClip));
    changed(row);
}

void SequenceModel::setOutPoint(int row, double seconds)
{
    if (row < 0 || row >= m_clips.size())
        return;
    Clip &c = m_clips[row];
    c.out = std::clamp(seconds, std::min(c.duration, c.in + kMinClip), c.duration);
    changed(row);
}

QVariantList SequenceModel::segments() const
{
    QVariantList list;
    for (const Clip &c : m_clips)
        list.append(QVariantMap{{QStringLiteral("path"), c.path}, {QStringLiteral("start"), c.in}, {QStringLiteral("end"), c.out}});
    return list;
}

double SequenceModel::totalDuration() const
{
    double total = 0;
    for (const Clip &c : m_clips)
        total += c.out - c.in;
    return total;
}

void SequenceModel::changed(int row)
{
    const QModelIndex idx = index(row);
    emit dataChanged(idx, idx);
    ++m_revision;
    emit revisionChanged();
}
