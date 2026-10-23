#include "MediaLibrary.h"

#include "Tools.h"

#include <QDirIterator>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QRegularExpression>
#include <QSet>

#include <algorithm>
#include <memory>

namespace {

// Two probe processes keep a low-end CPU responsive while still overlapping disk I/O.
constexpr int kMaxProbeProcesses = 2;
constexpr int kProbeBatch = 8;

const QSet<QString> kVideoSuffixes{
    QStringLiteral("mp4"), QStringLiteral("m4v"), QStringLiteral("mov"), QStringLiteral("mkv"),
    QStringLiteral("webm"), QStringLiteral("avi"), QStringLiteral("mts"), QStringLiteral("m2ts"),
    QStringLiteral("ts"), QStringLiteral("mpg"), QStringLiteral("mpeg"), QStringLiteral("mxf"),
    QStringLiteral("wmv"), QStringLiteral("flv"), QStringLiteral("3gp"),
};

QString statusText(MediaLibrary::Status status)
{
    switch (status) {
    case MediaLibrary::Probing: return QStringLiteral("Analyzing");
    case MediaLibrary::Ready: return QStringLiteral("Ready");
    case MediaLibrary::NeedsRemux: return QStringLiteral("Needs remux");
    case MediaLibrary::NeedsAudio: return QStringLiteral("Needs audio fix");
    case MediaLibrary::NeedsTranscode: return QStringLiteral("Needs re-encode");
    case MediaLibrary::Queued: return QStringLiteral("Queued");
    case MediaLibrary::Converting: return QStringLiteral("Converting");
    case MediaLibrary::Converted: return QStringLiteral("Converted");
    case MediaLibrary::Failed: return QStringLiteral("Failed");
    case MediaLibrary::Cancelled: return QStringLiteral("Cancelled");
    }
    return {};
}

bool isFixable(MediaLibrary::Status status)
{
    return status == MediaLibrary::NeedsRemux || status == MediaLibrary::NeedsAudio
        || status == MediaLibrary::NeedsTranscode || status == MediaLibrary::Cancelled;
}

} // namespace

MediaLibrary::MediaLibrary(QObject *parent)
    : QAbstractListModel(parent)
{
}

int MediaLibrary::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : int(m_items.size());
}

QVariant MediaLibrary::data(const QModelIndex &index, int role) const
{
    if (!index.isValid() || index.row() >= m_items.size())
        return {};
    const Item &it = m_items.at(index.row());
    switch (role) {
    case PathRole: return it.path;
    case NameRole: return it.name;
    case FolderRole: return it.folder;
    case SizeRole: return it.size;
    case DurationRole: return it.duration;
    case ResolutionRole:
        return it.width > 0 ? QStringLiteral("%1×%2").arg(it.width).arg(it.height) : QString();
    case CodecRole: return it.codec;
    case FpsRole: return it.fps;
    case StatusRole: return it.status;
    case StatusTextRole: return statusText(it.status);
    case ViolationsRole: return it.violations;
    case DetailsRole: return it.details;
    case ProgressRole: return it.progress;
    case OutputRole: return it.output;
    case ThumbRole:
        // Base64url keeps arbitrary paths intact through the image:// URL.
        return QStringLiteral("image://thumbs/")
            + QString::fromLatin1(it.path.toUtf8().toBase64(QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals));
    case CheckedRole: return it.checked;
    case FileUrlRole: return isLink(it.path) ? QUrl(it.path) : QUrl::fromLocalFile(it.path);
    case OutputUrlRole: return it.output.isEmpty() ? QUrl() : QUrl::fromLocalFile(it.output);
    case RemoteRole: return isLink(it.path);
    default: return {};
    }
}

QHash<int, QByteArray> MediaLibrary::roleNames() const
{
    return {
        {PathRole, "path"}, {NameRole, "name"}, {FolderRole, "folder"}, {SizeRole, "sizeBytes"},
        {DurationRole, "duration"}, {ResolutionRole, "resolution"}, {CodecRole, "codec"}, {FpsRole, "fps"},
        {StatusRole, "status"}, {StatusTextRole, "statusText"}, {ViolationsRole, "violations"},
        {DetailsRole, "details"}, {ProgressRole, "progress"}, {OutputRole, "output"},
        {ThumbRole, "thumb"}, {CheckedRole, "marked"}, {FileUrlRole, "fileUrl"}, {OutputUrlRole, "outputUrl"}, {RemoteRole, "remote"},
    };
}

void MediaLibrary::addUrls(const QList<QUrl> &urls)
{
    QStringList fresh;
    QSet<QString> seen;
    auto collect = [&](const QString &path) {
        const QString clean = QFileInfo(path).absoluteFilePath();
        if (m_rowByPath.contains(clean) || seen.contains(clean))
            return;
        seen.insert(clean);
        fresh << clean;
    };

    for (const QUrl &url : urls) {
        const QString local = url.toLocalFile();
        if (local.isEmpty())
            continue;
        const QFileInfo info(local);
        if (info.isDir()) {
            QDirIterator it(local, QDir::Files | QDir::Readable, QDirIterator::Subdirectories);
            while (it.hasNext()) {
                const QString file = it.next();
                if (kVideoSuffixes.contains(QFileInfo(file).suffix().toLower()))
                    collect(file);
            }
        } else if (info.isFile()) {
            // Explicitly chosen files are accepted whatever the extension; the probe decides.
            collect(info.absoluteFilePath());
        }
    }
    appendItems(fresh);
}

int MediaLibrary::addLinks(const QString &text)
{
    QStringList fresh;
    for (QString token : text.split(QRegularExpression(QStringLiteral("\\s+")), Qt::SkipEmptyParts)) {
        // Accept Markdown links like [label](https://...) and stray quotes/brackets, as remux_check.py did.
        if (const qsizetype paren = token.lastIndexOf(QStringLiteral("](")); paren >= 0 && token.endsWith(QLatin1Char(')')))
            token = token.mid(paren + 2, token.size() - paren - 3);
        while (!token.isEmpty() && QStringLiteral("*`<>\"'()[],").contains(token.back()))
            token.chop(1);
        while (!token.isEmpty() && QStringLiteral("*`<>\"'([").contains(token.front()))
            token.remove(0, 1);
        const QUrl url(token, QUrl::StrictMode);
        if (!url.isValid() || !isLink(token) || url.host().isEmpty())
            continue;
        if (!m_rowByPath.contains(token) && !fresh.contains(token))
            fresh << token;
    }
    appendItems(fresh);
    return int(fresh.size());
}

bool MediaLibrary::isLink(const QString &path)
{
    return path.startsWith(QLatin1String("http://"), Qt::CaseInsensitive)
        || path.startsWith(QLatin1String("https://"), Qt::CaseInsensitive);
}

void MediaLibrary::appendItems(const QStringList &fresh)
{
    if (fresh.isEmpty())
        return;
    const int first = int(m_items.size());
    beginInsertRows({}, first, first + int(fresh.size()) - 1);
    for (const QString &path : fresh) {
        Item item;
        item.path = path;
        if (isLink(path)) {
            const QUrl url(path);
            item.name = QUrl::fromPercentEncoding(url.fileName().toUtf8());
            if (item.name.isEmpty())
                item.name = url.host();
            item.folder = url.host() + url.adjusted(QUrl::RemoveFilename | QUrl::RemoveQuery).path();
        } else {
            const QFileInfo info(path);
            item.name = info.fileName();
            item.folder = info.absolutePath();
            item.size = info.size();
        }
        m_rowByPath.insert(path, int(m_items.size()));
        m_items.append(std::move(item));
    }
    endInsertRows();
    emit countChanged();
    ++m_revision;
    emit revisionChanged();
    enqueueProbe(fresh);
}

void MediaLibrary::removeAt(int row)
{
    if (m_locked || row < 0 || row >= m_items.size())
        return;
    const bool wasChecked = m_items.at(row).checked;
    beginRemoveRows({}, row, row);
    m_probeQueue.removeAll(m_items.at(row).path);
    m_items.removeAt(row);
    endRemoveRows();
    m_rowByPath.clear();
    for (int i = 0; i < m_items.size(); ++i)
        m_rowByPath.insert(m_items.at(i).path, i);
    emit countChanged();
    if (wasChecked)
        emit checkedCountChanged();
    ++m_revision;
    emit revisionChanged();
}

void MediaLibrary::clear()
{
    if (m_locked || m_items.isEmpty())
        return;
    beginResetModel();
    m_items.clear();
    m_rowByPath.clear();
    m_probeQueue.clear();
    ++m_generation; // results from probes still running are now irrelevant
    endResetModel();
    emit countChanged();
    emit checkedCountChanged();
    emit probingChanged();
    ++m_revision;
    emit revisionChanged();
}

void MediaLibrary::setChecked(int row, bool checked)
{
    if (row < 0 || row >= m_items.size() || m_items[row].checked == checked)
        return;
    m_items[row].checked = checked;
    touch(row);
    emit checkedCountChanged();
}

void MediaLibrary::setAllChecked(bool checked)
{
    if (m_items.isEmpty())
        return;
    for (Item &item : m_items)
        item.checked = checked;
    emit dataChanged(index(0), index(int(m_items.size()) - 1), {CheckedRole});
    emit checkedCountChanged();
    ++m_revision;
    emit revisionChanged();
}

QVariantMap MediaLibrary::get(int row) const
{
    QVariantMap map;
    if (row < 0 || row >= m_items.size())
        return map;
    const QModelIndex idx = index(row);
    const auto names = roleNames();
    for (auto it = names.cbegin(); it != names.cend(); ++it)
        map.insert(QString::fromLatin1(it.value()), data(idx, it.key()));
    return map;
}

int MediaLibrary::checkedCount() const
{
    return int(std::count_if(m_items.cbegin(), m_items.cend(), [](const Item &i) { return i.checked; }));
}

int MediaLibrary::fixableCount() const
{
    return int(std::count_if(m_items.cbegin(), m_items.cend(), [](const Item &i) { return isFixable(i.status); }));
}

void MediaLibrary::setPreset(const QString &preset)
{
    if (m_locked || preset == m_preset)
        return;
    m_preset = preset;
    emit presetChanged();
    // The plan depends on the preset, so every file is re-judged.
    ++m_generation;
    m_probeQueue.clear();
    QStringList all;
    for (int i = 0; i < m_items.size(); ++i) {
        Item &item = m_items[i];
        if (item.status == Converted)
            continue;
        item.status = Probing;
        item.violations.clear();
        item.details.clear();
        all << item.path;
        touch(i);
    }
    enqueueProbe(all);
}

void MediaLibrary::setLocked(bool locked)
{
    if (locked == m_locked)
        return;
    m_locked = locked;
    emit lockedChanged();
}

QStringList MediaLibrary::pathsToConvert() const
{
    QStringList checked;
    QStringList all;
    for (const Item &item : m_items) {
        if (!isFixable(item.status))
            continue;
        all << item.path;
        if (item.checked)
            checked << item.path;
    }
    // With nothing selected, "Convert" means every file that needs it.
    return checkedCount() > 0 ? checked : all;
}

void MediaLibrary::setJobState(const QString &path, Status status, double progress,
                               const QString &details, const QString &output)
{
    const int row = rowForPath(path);
    if (row < 0)
        return;
    Item &item = m_items[row];
    item.status = status;
    item.progress = progress;
    if (!details.isEmpty())
        item.details = details;
    if (!output.isEmpty())
        item.output = output;
    touch(row);
}

int MediaLibrary::rowForPath(const QString &path) const
{
    return m_rowByPath.value(path, -1);
}

MediaLibrary::Status MediaLibrary::statusOf(const QString &path) const
{
    const int row = rowForPath(path);
    return row >= 0 ? m_items.at(row).status : Failed;
}

void MediaLibrary::touch(int row)
{
    const QModelIndex idx = index(row);
    emit dataChanged(idx, idx);
    ++m_revision;
    emit revisionChanged();
}

void MediaLibrary::enqueueProbe(const QStringList &paths)
{
    m_probeQueue << paths;
    pumpProbes();
    emit probingChanged();
}

void MediaLibrary::pumpProbes()
{
    const QString cli = Tools::senseCli();
    if (cli.isEmpty()) {
        failProbing(m_probeQueue, tr("sense-cli was not found next to the app or on PATH"));
        m_probeQueue.clear();
        return;
    }

    while (m_activeProbes < kMaxProbeProcesses && !m_probeQueue.isEmpty()) {
        const QStringList batch = m_probeQueue.mid(0, kProbeBatch);
        m_probeQueue.remove(0, batch.size());

        auto *proc = new QProcess(this);
        proc->setProcessEnvironment(Tools::environment());
        const int generation = m_generation;
        auto pending = std::make_shared<QByteArray>();

        auto drain = [this, proc, pending, generation](bool final) {
            pending->append(proc->readAllStandardOutput());
            qsizetype newline;
            while ((newline = pending->indexOf('\n')) >= 0) {
                const QByteArray line = pending->left(newline);
                pending->remove(0, newline + 1);
                if (generation == m_generation)
                    handleProbeLine(line);
            }
            if (final && !pending->isEmpty() && generation == m_generation)
                handleProbeLine(*pending);
        };

        connect(proc, &QProcess::readyReadStandardOutput, this, [drain] { drain(false); });
        auto finish = [this, proc, batch, generation, drain](const QString &failure) {
            drain(true);
            if (generation == m_generation) {
                // Anything the sidecar did not report on is marked failed rather than left spinning.
                QStringList unanswered;
                for (const QString &path : batch) {
                    const int row = rowForPath(path);
                    if (row >= 0 && m_items.at(row).status == Probing)
                        unanswered << path;
                }
                failProbing(unanswered, failure.isEmpty() ? tr("Analysis did not complete") : failure);
            }
            proc->deleteLater();
            --m_activeProbes;
            pumpProbes();
            emit probingChanged();
        };
        connect(proc, &QProcess::finished, this, [proc, finish](int code, QProcess::ExitStatus status) {
            QString failure;
            if (status == QProcess::CrashExit || code != 0)
                failure = QString::fromLocal8Bit(proc->readAllStandardError()).trimmed().right(400);
            finish(failure);
        });
        connect(proc, &QProcess::errorOccurred, this, [proc, finish](QProcess::ProcessError error) {
            // finished() is never emitted when the process could not start.
            if (error == QProcess::FailedToStart)
                finish(tr("Could not start sense-cli: %1").arg(proc->errorString()));
        });

        ++m_activeProbes;
        proc->start(cli, QStringList{QStringLiteral("probe"), QStringLiteral("--preset"), m_preset} + batch);
    }
}

void MediaLibrary::handleProbeLine(const QByteArray &line)
{
    const QJsonObject obj = QJsonDocument::fromJson(line).object();
    const int row = rowForPath(obj.value(QStringLiteral("source")).toString());
    if (row < 0)
        return;
    Item &item = m_items[row];

    const QJsonObject info = obj.value(QStringLiteral("info")).toObject();
    item.duration = info.value(QStringLiteral("duration")).toDouble();
    if (const qint64 size = info.value(QStringLiteral("size")).toInteger(); size > 0)
        item.size = size;
    const QJsonObject video = info.value(QStringLiteral("video")).toObject();
    item.codec = video.value(QStringLiteral("codec")).toString();
    item.fps = video.value(QStringLiteral("avg_fps")).toDouble();
    const int w = video.value(QStringLiteral("width")).toInt();
    const int h = video.value(QStringLiteral("height")).toInt();
    const bool quarterTurn = video.value(QStringLiteral("rotation")).toInt() % 180 == 90;
    item.width = quarterTurn ? h : w;
    item.height = quarterTurn ? w : h;

    if (obj.contains(QStringLiteral("error"))) {
        item.status = Failed;
        item.details = obj.value(QStringLiteral("error")).toString();
        touch(row);
        return;
    }

    const QJsonObject plan = obj.value(QStringLiteral("plan")).toObject();
    const QString kind = plan.value(QStringLiteral("kind")).toString();
    item.status = kind == QLatin1String("no_op")             ? Ready
                  : kind == QLatin1String("remux")           ? NeedsRemux
                  : kind == QLatin1String("audio_transcode") ? NeedsAudio
                                                             : NeedsTranscode;
    item.violations.clear();
    int blocking = 0;
    for (const QJsonValue &v : plan.value(QStringLiteral("violations")).toArray()) {
        const QJsonObject vo = v.toObject();
        const bool isBlocking = vo.value(QStringLiteral("fix")).toString() != QLatin1String("advisory");
        blocking += isBlocking ? 1 : 0;
        item.violations.append(QVariantMap{
            {QStringLiteral("message"), vo.value(QStringLiteral("message")).toString()},
            {QStringLiteral("blocking"), isBlocking},
            {QStringLiteral("fix"), vo.value(QStringLiteral("fix")).toString()},
        });
    }
    item.details = blocking == 0 ? tr("Meets the StepOut Ready spec") : tr("%n issue(s) to fix", nullptr, blocking);
    touch(row);
}

void MediaLibrary::failProbing(const QStringList &paths, const QString &reason)
{
    for (const QString &path : paths) {
        const int row = rowForPath(path);
        if (row < 0)
            continue;
        m_items[row].status = Failed;
        m_items[row].details = reason;
        touch(row);
    }
}
