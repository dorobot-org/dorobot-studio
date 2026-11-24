"""Move completed worker evidence as a bounded, checksum-verified archive."""

import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile
from zipfile import ZIP_DEFLATED, ZipFile

from worker import save

CHUNK = 2 * 1024 * 1024


def metadata(job):
    archive = job / "trial.zip"
    receipt = job / "trial-archive.json"
    with (job / "archive.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if archive.is_file() and receipt.is_file():
            return json.loads(receipt.read_bytes())
        source = job / "trial"
        if not source.is_dir():
            raise ValueError("This simulation has no trial artifacts")
        tmp = job / "trial.zip.tmp"
        with ZipFile(tmp, "w", compression=ZIP_DEFLATED, compresslevel=3) as output:
            for path in sorted(source.rglob("*")):
                if path.is_file() and not path.is_symlink():
                    output.write(path, path.relative_to(source))
        tmp.replace(archive)
        with archive.open("rb") as f:
            info = dict(
                bytes=archive.stat().st_size,
                sha256=hashlib.file_digest(f, "sha256").hexdigest(),
                files=len(output.namelist()),
            )
        save(receipt, info)
        return info


def piece(job, request):
    info = json.loads((job / "trial-archive.json").read_bytes())
    offset = request["offset"]
    if (
        request["sha256"] != info["sha256"]
        or type(offset) is not int
        or not 0 <= offset < info["bytes"]
    ):
        raise ValueError("Choose an offset inside the selected trial archive")
    with (job / "trial.zip").open("rb") as archive:
        archive.seek(offset)
        data = archive.read(CHUNK)
    if len(data) != min(CHUNK, info["bytes"] - offset):
        raise ValueError("The trial archive size changed")
    return dict(offset=offset, data=base64.b64encode(data).decode())


def download(send, node, ident, destination):
    info = send(node, dict(op="archive", id=ident))
    if destination.exists():
        raise ValueError("Choose a new output archive")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            dir=destination.parent, delete=False
        ) as stream:
            temporary = Path(stream.name)
            checksum = hashlib.sha256()
            while stream.tell() < info["bytes"]:
                pos = stream.tell()
                part = send(
                    node,
                    dict(
                        op="read-archive", id=ident, offset=pos, sha256=info["sha256"]
                    ),
                )
                data = base64.b64decode(part["data"], validate=True)
                if part["offset"] != pos or len(data) != min(
                    CHUNK, info["bytes"] - pos
                ):
                    raise ValueError("Incomplete trial archive transfer")
                stream.write(data)
                checksum.update(data)
            if checksum.hexdigest() != info["sha256"]:
                raise ValueError("Downloaded evidence differs from the worker archive")
            stream.flush()
            os.fsync(stream.fileno())
        os.link(temporary, destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return dict(**info, output=str(destination))
