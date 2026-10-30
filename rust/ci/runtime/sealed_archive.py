"""Restore canonical runtime members without changing their sealed file modes."""
import os
import shutil
import stat
import tarfile


def extract_sealed_member(payload, member, root):
    # Retain stdlib containment/link checks and ownership stripping. Validate
    # the original header too: filtering must not silently sanitize identity.
    filtered = tarfile.data_filter(member, root)
    parts = member.name.split("/")
    if (not member.name or len(member.name) > 4096 or "\\" in member.name or "\0" in member.name
            or any(part in ("", ".", "..") for part in parts)):
        raise tarfile.FilterError(f"noncanonical sealed path: {member.name!r}")
    if member.type not in (tarfile.REGTYPE, tarfile.SYMTYPE) or member.sparse is not None:
        raise tarfile.FilterError(f"unsupported sealed type: {member.name}")
    if not 0 <= member.mode <= 0o777:
        raise tarfile.FilterError(f"unsafe sealed mode: {member.name}")
    if member.issym() and (member.size != 0 or not member.linkname
                           or "\\" in member.linkname or "\0" in member.linkname
                           or os.path.normpath(member.linkname) != member.linkname
                           or filtered.linkname != member.linkname):
        raise tarfile.FilterError(f"noncanonical sealed link: {member.name}")

    # Walk using directory descriptors so neither extraction nor fchmod can
    # traverse an existing or previously extracted symlink, even within root.
    directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    parent = os.open(root, directory_flags)
    try:
        for part in parts[:-1]:
            try:
                os.mkdir(part, 0o755, dir_fd=parent)
            except FileExistsError:
                pass
            child = os.open(part, directory_flags, dir_fd=parent)
            os.close(parent)
            parent = child
        if member.issym():
            os.symlink(filtered.linkname, parts[-1], dir_fd=parent)
            return
        flags = os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK
        fd = os.open(parts[-1], flags, 0o600, dir_fd=parent)
        with os.fdopen(fd, "wb") as destination:
            info = os.fstat(destination.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise tarfile.FilterError(f"sealed destination is not an unaliased regular file: {member.name}")
            os.ftruncate(destination.fileno(), 0)
            with payload.extractfile(filtered) as source:
                shutil.copyfileobj(source, destination)
            destination.flush()
            os.utime(destination.fileno(), (filtered.mtime, filtered.mtime))
            # data_filter deliberately changes 0664/0444/0555. Only validated
            # ordinary permission bits are restored, on this regular fd.
            os.fchmod(destination.fileno(), member.mode)
    finally:
        os.close(parent)
