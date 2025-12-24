#!/usr/bin/env python3
"""
Git tag release script to simplify creating git tags with changelog since last release.
Copyright (c) 2025 Unfolded Circle.
"""

import os
import subprocess
import sys
import tempfile
import re
import json


def run_command(command, check=True):
    """Run a shell command and return the output."""
    try:
        result = subprocess.run(command, shell=True, check=check, capture_output=True, text=True)
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        print(f"Error running command: {command}")
        print(f"Stdout: {e.stdout}")
        print(f"Stderr: {e.stderr}")
        if check:
            sys.exit(1)
        return None


def get_latest_tag():
    """Get the latest git tag."""
    return run_command("git describe --tags --abbrev=0", check=False)


def get_commits_since(tag):
    """Get commits since the specified tag."""
    format_str = '--pretty=format:"%s|%an"'
    if tag:
        return run_command(f"git log {tag}..HEAD {format_str}")
    else:
        return run_command(f"git log {format_str}")


def is_valid_semver(tag):
    """Check if the version is a valid semver format (X.Y.Z)."""
    return re.match(r"^\d+\.\d+\.\d+$", tag) is not None


def check_version_match(version):
    """Check if the version matches the version in Cargo.toml."""
    cargo_toml_path = os.path.join(os.path.dirname(__file__), "..", "Cargo.toml")
    if not os.path.exists(cargo_toml_path):
        print(f"Error: {cargo_toml_path} not found.")
        sys.exit(1)

    cargo_version = None
    with open(cargo_toml_path, "r") as f:
        in_package = False
        for line in f:
            line = line.strip()
            if line == "[package]":
                in_package = True
            elif line.startswith("[") and line.endswith("]"):
                in_package = False
            
            if in_package:
                match = re.match(r'^version\s*=\s*"(.*?)"', line)
                if match:
                    cargo_version = match.group(1)
                    break

    if not cargo_version:
        print(f"Error: Could not find version in [package] section of {cargo_toml_path}")
        sys.exit(1)

    if cargo_version != version:
        print(f"Error: Provided version '{version}' does not match Cargo.toml version '{cargo_version}'.")
        sys.exit(1)


import argparse


def main():
    parser = argparse.ArgumentParser(description="Git tag script to simplify creating git tags.")
    parser.add_argument("version", help="The new version in semver format (e.g., 0.21.0)")
    parser.add_argument("--dry-run", action="store_true", help="Do not create or push the tag")
    args = parser.parse_args()

    version = args.version
    dry_run = args.dry_run
    if not is_valid_semver(version):
        print(f"Error: Version '{version}' is not in valid semver format (X.Y.Z)")
        sys.exit(1)

    check_version_match(version)

    new_tag = f"v{version}"

    # Check if tag already exists
    existing_tags = run_command("git tag").split("\n")
    if new_tag in existing_tags:
        print(f"Error: Tag '{new_tag}' already exists.")
        sys.exit(1)

    latest_tag = get_latest_tag()
    print(f"Latest tag: {latest_tag}")

    if latest_tag:
        commits = get_commits_since(latest_tag)
    else:
        commits = get_commits_since(None)

    if not commits:
        print("No commits since last tag.")
        sys.exit(0)

    # Process commits
    formatted_pr_commits = []
    formatted_other_commits = []

    for line in commits.split("\n"):
        if not line:
            continue
        parts = line.split("|")
        if len(parts) < 2:
            continue
        message = parts[0]
        _author = parts[1]

        # Extract PR number if present
        pr_match = re.search(r"\(#(\d+)\)", message)
        if pr_match:
            pr_num = pr_match.group(1)
            # Remove the (#num) part from message
            clean_message = re.sub(r"\s*\(#\d+\)", "", message).strip()
            formatted_line = f"{clean_message} in #{pr_num}"
            formatted_pr_commits.append(formatted_line)
        else:
            formatted_line = f"{message}"
            formatted_other_commits.append(formatted_line)

    initial_message = f"Release {new_tag}\n\n"
    if formatted_pr_commits:
        initial_message += "Pull Requests:\n"
        for line in formatted_pr_commits:
            initial_message += f"- {line}\n"
        initial_message += "\n"

    if formatted_other_commits:
        initial_message += "Other Changes:\n"
        for line in formatted_other_commits:
            initial_message += f"- {line}\n"

    # Create temporary file for editing
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as tmp:
        tmp.write(initial_message.encode("utf-8"))
        tmp_path = tmp.name

    editor = os.environ.get("EDITOR", "vim")
    subprocess.call([editor, tmp_path])

    with open(tmp_path, "r") as f:
        tag_message = f.read().strip()

    os.unlink(tmp_path)

    if not tag_message:
        print("Tag message is empty. Aborting.")
        sys.exit(1)

    print("\n--- Tag Message ---")
    print(tag_message)
    print("-------------------\n")

    confirm = input(f"Create and push tag {new_tag}? (y/n): ")
    if confirm.lower() == "y":
        if dry_run:
            print(f"[DRY-RUN] Would create annotated tag: {new_tag}")
            print(f"[DRY-RUN] Would push tag {new_tag} to origin.")
        else:
            # Create annotated tag
            run_command(f'git tag -a {new_tag} -m "{tag_message}"')
            print(f"Tag {new_tag} created locally.")

            # Push tag
            run_command(f"git push origin {new_tag}")
            print(f"Tag {new_tag} pushed to origin.")
    else:
        print("Aborted.")


if __name__ == "__main__":
    main()
