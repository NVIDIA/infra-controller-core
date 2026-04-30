#!/usr/bin/env python3
# SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


APACHE_LICENSE = "SPDX-License-Identifier: Apache-2.0"
PROPRIETARY_LICENSE = "SPDX-License-Identifier: " + "LicenseRef-NvidiaProprietary"
DEFAULT_COPYRIGHT = "Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved."
HEADER_WINDOW = 4096

BLOCK_COMMENT_EXTENSIONS = {
    ".c",
    ".cc",
    ".cpp",
    ".cs",
    ".cu",
    ".cuh",
    ".go",
    ".h",
    ".hpp",
    ".java",
    ".js",
    ".jsx",
    ".rs",
    ".ts",
    ".tsx",
}
HASH_COMMENT_EXTENSIONS = {".py", ".sh", ".bash", ".zsh"}
EXCLUDED_DIRS = {
    ".git",
    ".cache",
    "build",
    "dist",
    "node_modules",
    "target",
    "third-party",
    "third_party",
    "vendor",
}
EXCLUDED_PREFIXES = (
    "ipam/",
    "temporal-helm/",
)
EXCLUDED_FILE_SUFFIXES = (
    ".dockerignore",
    ".expected",
    ".example",
    ".tmpl",
)

COPYRIGHT_RE = re.compile(r"SPDX-FileCopyrightText:\s*(.+)")
BLOCK_PROPRIETARY_RE = re.compile(
    r"\A/\*.*?SPDX-License-Identifier:\s*" + "LicenseRef-NvidiaProprietary" + r".*?\*/\s*",
    re.DOTALL,
)
HASH_PROPRIETARY_RE = re.compile(
    r"\A(?P<shebang>#![^\n]*\n)?(?P<header>(?:#[^\n]*(?:\n|$)){2,40})",
    re.DOTALL,
)


def tracked_files(repo: Path) -> list[Path]:
    output = subprocess.check_output(["git", "ls-files"], cwd=repo, text=True)
    return [Path(line) for line in output.splitlines()]


def is_dockerfile(path: Path) -> bool:
    return path.name == "Dockerfile" or path.name.startswith("Dockerfile.")


def has_shebang(path: Path) -> bool:
    try:
        return path.read_bytes().startswith(b"#!")
    except OSError:
        return False


def is_generated(text: str) -> bool:
    header = text[:HEADER_WINDOW]
    return "Code generated" in header or "DO NOT EDIT" in header or "@generated" in header


def is_candidate(repo: Path, path: Path) -> bool:
    if any(part in EXCLUDED_DIRS for part in path.parts):
        return False
    path_text = path.as_posix()
    if any(path_text.startswith(prefix) for prefix in EXCLUDED_PREFIXES):
        return False
    if path_text.endswith(EXCLUDED_FILE_SUFFIXES):
        return False

    full_path = repo / path
    return (
        path.suffix in BLOCK_COMMENT_EXTENSIONS
        or path.suffix in HASH_COMMENT_EXTENSIONS
        or is_dockerfile(path)
        or has_shebang(full_path)
    )


def comment_style(path: Path) -> str:
    if path.suffix in BLOCK_COMMENT_EXTENSIONS:
        return "block"
    return "hash"


def copyright_text(text: str) -> str:
    match = COPYRIGHT_RE.search(text[:HEADER_WINDOW])
    if match:
        return match.group(1).strip()
    return DEFAULT_COPYRIGHT


def block_header(copyright: str) -> str:
    return f"""/*
 * SPDX-FileCopyrightText: {copyright}
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

"""


def hash_header(copyright: str) -> str:
    return f"""# SPDX-FileCopyrightText: {copyright}
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""


def strip_proprietary_hash_header(text: str) -> tuple[str, str]:
    match = HASH_PROPRIETARY_RE.match(text)
    if not match or PROPRIETARY_LICENSE not in match.group("header"):
        return "", text

    shebang = match.group("shebang") or ""
    return shebang, text[match.end() :]


def apache_header(path: Path, text: str) -> str:
    copyright = copyright_text(text)
    if comment_style(path) == "block":
        return block_header(copyright)
    return hash_header(copyright)


def fix_text(path: Path, text: str) -> str:
    header = apache_header(path, text)

    if comment_style(path) == "block":
        match = BLOCK_PROPRIETARY_RE.match(text)
        if match:
            return header + text[match.end() :].lstrip("\n")
        return header + text.lstrip("\n")

    shebang = ""
    body = text
    if text.startswith("#!"):
        shebang, _, body = text.partition("\n")
        shebang += "\n"

    proprietary_shebang, stripped_body = strip_proprietary_hash_header(text)
    if proprietary_shebang:
        return proprietary_shebang + header + stripped_body.lstrip("\n")

    return shebang + header + body.lstrip("\n")


def scan(repo: Path, *, fix: bool) -> int:
    missing: list[Path] = []
    proprietary: list[Path] = []
    fixed: list[Path] = []

    for path in tracked_files(repo):
        if not is_candidate(repo, path):
            continue

        full_path = repo / path
        text = full_path.read_text(errors="ignore")

        header = text[:HEADER_WINDOW]
        if PROPRIETARY_LICENSE in header:
            proprietary.append(path)
        elif APACHE_LICENSE not in header:
            missing.append(path)
        else:
            continue

        if fix:
            full_path.write_text(fix_text(path, text))
            fixed.append(path)

    if fixed:
        print(f"Updated Apache-2.0 headers in {len(fixed)} files.")
        missing = []
        proprietary = []

    if missing or proprietary:
        if missing:
            print(f"Files missing Apache-2.0 source headers: {len(missing)}")
            for path in missing:
                print(f"  {path}")
        if proprietary:
            print(f"Files with proprietary source headers: {len(proprietary)}")
            for path in proprietary:
                print(f"  {path}")
        return 1 if not fix else 0

    print("All checked source files have Apache-2.0 headers.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Check NVIDIA source files for Apache-2.0 headers.")
    parser.add_argument("--fix", action="store_true", help="Insert or replace Apache-2.0 source headers.")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    return scan(repo, fix=args.fix)


if __name__ == "__main__":
    sys.exit(main())
