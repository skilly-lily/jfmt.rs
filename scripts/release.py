#! /usr/bin/env python

from __future__ import annotations
from contextlib import contextmanager
import functools
from math import floor
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
from typing import Callable, Optional, Union, cast

import requests
from requests import Response
import tomlkit

GH_OWNER = "scruffystuffs"
GH_REPO = "jfmt.rs"
TIMEOUT = 15 * 60  # 15 min
MIN_BUILDTIME = 4 * 60  # 4 min
WORKFLOW_POLL_SECONDS = 10
WORKFLOW_NAME = "Publish"
WORKFLOW_FETCH_DELAY = 10


def with_progress(message: str):
    def progress_dec(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            eprint(message, "...", sep="")
            rv = func(*args, **kwargs)
            eprint(message, ": Complete!", sep="")
            return rv

        return wrapper

    return progress_dec


class Github:
    def __init__(self, token: str) -> None:
        self.token = token
        self._workflow_id: Optional[int] = None

    @classmethod
    def from_env(cls) -> Github:
        token = os.getenv("GITHUB_TOKEN", None)
        if not token:
            raise ValueError("Need github token, none set")
        return cls(token)

    @property
    def workflow_id(self) -> int:
        if not self._workflow_id:
            self._workflow_id = self._fetch_workflow_id()
        return cast(int, self._workflow_id)

    def wait_for_commit_workflow_success(self, commit: str):
        run_id = self._get_run_id_for_commit(commit)
        self._wait_for_workflow_run(run_id)

    def _wait_for_workflow_run(self, run_id: int):
        eprint("Builds take a while, we start polling after a minimum wait time...")
        hold_with_sticky("Holding until build delay has elapsed", MIN_BUILDTIME)
        for _ in timeout_iter(TIMEOUT):
            run = self._get_via_repo("actions", "runs", str(run_id)).json()
            status = run["status"]
            if not status == "completed":
                eprint(
                    f"Workflow not completed, current status: {status}, sleeping for {WORKFLOW_POLL_SECONDS} seconds..."
                )
                time.sleep(WORKFLOW_POLL_SECONDS)
                continue
            conclusion = run["conclusion"]
            if not conclusion == "success":
                raise AssertionError(
                    f"Workflow run failed with conclusion: {conclusion}"
                )
            return

        raise AssertionError("commit workflow timed out")

    def _get_run_id_for_commit(self, commit: str) -> int:
        return self._get_run_id_by(lambda run: run["head_sha"] == commit)

    def _get_run_id_for_tag(self, tag: str) -> int:
        return self._get_run_id_by(lambda run: run["head_branch"] == tag)

    def _get_run_id_by(self, func: Callable[[dict], bool]) -> int:
        backoff = 1
        hold_with_sticky("Delaying run_id fetch to avoid conflicts", WORKFLOW_FETCH_DELAY)
        for _retry in range(4):
            all_runs = self._get_via_repo(
                "actions", "workflows", str(self.workflow_id), "runs"
            ).json()["workflow_runs"]
            runs = list(filter(func, all_runs))
            if not runs:
                eprint(f"No workflow runs found, retrying in {backoff}s.")
                time.sleep(backoff)
                backoff *= 2
                continue
            if len(runs) == 1:
                return runs[0]["id"]
            return max(runs, key=lambda run: run["run_number"])["id"]
        raise AssertionError("No workflow runs found after 4 tries.")

    def wait_for_release_workflow(self, version: str):
        run_id = self._get_run_id_for_tag(version)
        self._wait_for_workflow_run(run_id)

    @with_progress("Fetching workflow id")
    def _fetch_workflow_id(self) -> int:
        jdata = self._get_via_repo("actions", "workflows").json()
        for item in jdata["workflows"]:
            if item["name"] == WORKFLOW_NAME:
                return item["id"]
        raise AssertionError("No workflow matched the predicate.")

    def _get_via_repo(self, *args) -> Response:
        base = f"https://api.github.com/repos/{GH_OWNER}/{GH_REPO}"
        uri = "/".join([base, *args])
        headers = {
            "Accept": "application/vnd.github.v3+json",
            "Authorization": f"token {self.token}",
        }
        eprint("API call:", uri)
        resp = requests.get(uri, headers=headers)
        resp.raise_for_status()
        return resp

    def fetch_latest_release(self) -> dict:
        return self._get_via_repo("releases", "latest").json()


class ReleaseState:
    def __init__(self) -> None:
        self.did_commit_run = False
        self.did_push_commit = False
        self.did_release = False


class ReleaseActor:
    def __init__(self, version_arg: str) -> None:
        self._version_arg = version_arg
        self._github: Optional[Github] = None
        self._root: Optional[Path] = None
        self._commit_hash: Optional[str] = None
        self._new_version: Optional[str] = None
        self._state = ReleaseState()

    @property
    def github(self) -> Github:
        if self._github is None:
            self._github = Github.from_env()
        return self._github

    @property
    def root(self) -> Path:
        if self._root is None:
            self._root = find_root()
        return self._root

    @property
    def commit_hash(self) -> str:
        if not self._state.did_commit_run:
            raise AssertionError("Tried to read commit hash before committing.")
        if self._commit_hash is None:
            self._commit_hash = cast(
                str, git_run("rev-parse", "--verify", "HEAD", capture=True)
            )
        return self._commit_hash

    @property
    def new_version(self) -> str:
        if self._new_version is None:
            self._new_version = determine_version(self._version_arg, self.root)
        return self._new_version

    def update_cargo_toml(self):
        cargotoml = self.root / "Cargo.toml"
        backup = cargotoml.with_suffix(".toml.bak")
        with progress("Backing up cargo.toml"):
            shutil.copy(cargotoml, backup)
        with progress("Editing cargo.toml version"):
            with cargotoml.open("r") as tfp:
                tdata = tomlkit.load(tfp)
            tdata.setdefault("package", {})["version"] = self.new_version
            with cargotoml.open("w") as tfp:
                tomlkit.dump(tdata, tfp)

        pager_run(str(cargotoml))
        if not confirm("Does this Cargo.toml file look right to you?"):
            # Restore the backup
            backup.rename(cargotoml)
            raise Exception("Cargo.toml edits rejected by user.")
        backup.unlink()
        cargo_run("generate-lockfile")

    @with_progress("Updating changelog")
    def update_changelog(self):
        changie_run("batch", self.new_version)
        changie_run("merge")

    def commit_release(self):
        commit_changes(self.new_version, self.root)
        self._state.did_commit_run = True

    def push_commit(self):
        if not self._state.did_commit_run:
            raise AssertionError("Tried to push before committing.")
        with progress("Running commit workflow"):
            git_run("push", "origin", "master")
            self.github.wait_for_commit_workflow_success(self.commit_hash)
        self._state.did_push_commit = True

    def run_release(self):
        if not self._state.did_push_commit:
            raise AssertionError("Tried to tag before committing to master")
        tag = f"v{self.new_version}"
        git_run("tag", "-am", f"Release {tag}", tag)
        git_run("push", "origin", tag)
        self.github.wait_for_release_workflow(tag)
        self.check_release()

    def check_release(self):
        release = self.github.fetch_latest_release()
        if not release["tag_name"] == f"v{self.new_version}":
            raise AssertionError(
                f"Latest release is not the expected release tag, found: {release['tag_name']}"
            )
        self._state.did_release = True

    def cargo_publish(self):
        if not self._state.did_release:
            raise AssertionError("Tried to publish cargo package before release.")
        cargo_run("publish")


def _proc_run(
    *args, procname: str, capture: bool = False, allow_err: bool = False
) -> Union[str, bool]:
    eprint("Running command:", *[procname, *args])
    try:
        proc = subprocess.run(
            [procname] + list(args), capture_output=capture, text=True, check=True
        )
    except subprocess.CalledProcessError as err:
        if allow_err:
            return False
        print(err.stderr, file=sys.stderr)
        sys.exit(1)
    else:
        if capture:
            return proc.stdout.strip()
        return True


def timeout_iter(timeout_seconds: int):
    start = time.monotonic()
    while True:
        yield
        now = time.monotonic()
        if now >= start + timeout_seconds:
            break


def pager_run(filename: str):
    pager = os.getenv("PAGER", "less")
    _proc_run(filename, procname=pager, allow_err=True)


def changie_run(
    *args, capture: bool = False, allow_err: bool = False
) -> Union[str, bool]:
    return _proc_run(*args, procname="changie", capture=capture, allow_err=allow_err)


def git_run(*args, capture: bool = False, allow_err: bool = False) -> Union[str, bool]:
    return _proc_run(*args, procname="git", capture=capture, allow_err=allow_err)


def cargo_run(
    *args, capture: bool = False, allow_err: bool = False
) -> Union[str, bool]:
    return _proc_run(*args, procname="cargo", capture=capture, allow_err=allow_err)


def confirm(msg: str) -> bool:
    answer = input(f"{msg} [y/N]: ")
    if not answer:
        return False
    answer = answer[0].lower()
    if answer == "y":
        return True
    if answer in ["n"]:
        return False
    print("Input unrecognized, aborting...", file=sys.stderr)
    sys.exit(1)


def determine_version(raw_arg: str, root: Path) -> str:
    # Determine release update level (major|minor|patch)
    version_arg = raw_arg.lower().strip()
    if version_arg in ["major", "minor", "patch"]:
        # Get next version from changie
        version = cast(str, changie_run("next", str(version_arg), capture=True)).strip()
    else:
        # Simple semver only
        match = re.match(r"^v?(\d+\.\d+\.\d+)$", version_arg)
        if not match:
            raise ValueError("Invalid version provided.")
        version = match.group(1)
    if (root / ".git" / "refs" / "tags" / f"v{version}").is_file():
        raise AssertionError(f"Tag for version already exists: {version}")
    return version

def hold_with_sticky(msg: str, seconds: int):
    delay = 0.5
    start = time.monotonic()
    elapsed = start
    line = ""
    while elapsed < start + seconds:
        remaining_seconds = floor(start + seconds - elapsed)
        line = f"{msg}: {remaining_seconds: >5}s remaining"
        eprint(line, end="\r")
        time.sleep(delay)
        elapsed = time.monotonic()
    eprint(" " * len(line), end="\r")

def eprint(*args, **kwargs):
    kwargs["file"] = sys.stderr
    print(*args, **kwargs)


@contextmanager
def progress(message: str):
    eprint(message, "...", sep="")
    yield
    eprint(message, ": Complete!", sep="")


def find_root() -> Path:
    candidate = Path().resolve()
    root = Path(candidate.root)
    while candidate != root:
        indicator = candidate / "Cargo.toml"
        if indicator.is_file():
            return candidate
        candidate = candidate.parent
    raise FileNotFoundError("Could not find Cargo.toml.")


def commit_changes(version: str, root: Path):
    file_list = map(
        str,
        [
            root / "Cargo.toml",
            root / "Cargo.lock",
            root / "changes" / f"{version}.md",
            root / "CHANGELOG.md",
            root / "changes" / "unreleased",
        ],
    )
    git_run("add", *file_list)
    git_run("commit", "-m", f"Release v{version}")


def parse_args() -> str:
    args = sys.argv[1:]
    if args:
        unused = args[1:]
        if unused:
            eprint("Unused command-line args:", *unused)
        return args[0]
    raise AssertionError(
        "No version argument: use major, minor, patch, or a semver version"
    )


def main():
    # Parse args
    version_arg = parse_args()
    actor = ReleaseActor(version_arg)

    # Determine package root
    # Determine new version
    # Update Cargo.toml version
    actor.update_cargo_toml()

    # Changelog batch
    # Changelog merge
    actor.update_changelog()

    # Commit with bump message
    actor.commit_release()

    # Push to master
    # Wait for master workflow to complete
    actor.push_commit()

    # Create git tag
    # Git tag push
    # Wait for github release
    # Confirm github release
    actor.run_release()

    # Cargo publish
    actor.cargo_publish()


if __name__ == "__main__":
    main()
