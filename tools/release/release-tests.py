#!/usr/bin/env python3
"""Release boundaries in disposable repositories; never create real releases."""
import importlib.util
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

REAL_RUN = subprocess.run

def fake_cliff(command, **kwargs):
    if command[0] == "git-cliff":
        return subprocess.CompletedProcess(command, 0)
    return REAL_RUN(command, **kwargs)


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release", ROOT / "tools/release/release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="amx-release-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "repo"
        self.root.mkdir()
        self.git("init", "-q")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Release Test")
        self.write("Cargo.toml", '[workspace.package]\nversion = "0.1.0"\n')
        self.write("Cargo.lock", 'version = 4\n')
        self.write("cliff.toml", (ROOT / "cliff.toml").read_text())
        self.write("docs/releases/v0.1.0.md", "# v0.1.0\n\nReviewed release notes.\n")
        self.commit("Initial work, no conventional prefix")

    def git(self, *args):
        return release.git(self.root, *args)

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-qm", message)
        return self.git("rev-parse", "HEAD")

    def tag(self, annotated=False):
        self.git("tag", *( ["-a", "v0.1.0", "-m", "Reviewed release"] if annotated else ["v0.1.0"]))
        return release.resolve(self.root, "v0.1.0")

    def test_strict_versions_and_names(self):
        for value in ("0.1.0", "1.2.3-rc.1", "1.2.3-alpha-beta+build.001"):
            self.assertEqual(release.version(value), value)
        for value in ("v1.2.3", "1.2", "01.2.3", "1.02.3", "1.2.03", "1.2.3-01", "1.2.3-", "1.2.3+", "1.2.3foo", "1.2.3\n", "1.2.3;echo x"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                release.version(value)
        for name in ("nightly-2026", "cf-pages-abc", "main", "--help", "v1.2.3junk"):
            with self.assertRaises(ValueError):
                release.tag_name(name)

    def test_publication_origin_accepts_only_unambiguous_github_repository(self):
        for url in ("https://github.com/j4ger/animatix.git", "git@github.com:j4ger/animatix.git",
                    "ssh://git@github.com/j4ger/animatix.git", "https://github.com/j4ger/animatix"):
            self.assertEqual(release.github_repository(url), "j4ger/animatix")
        for url in ("https://github.com.evil/j4ger/animatix", "/tmp/local.git", "https://token@github.com/j4ger/animatix",
                    "https://github.com/j4ger/animatix/extra", "https://other.example/j4ger/animatix"):
            with self.assertRaises(ValueError):
                release.github_repository(url)

    def test_first_draft_ignores_operational_tags_and_does_not_create_release(self):
        self.git("tag", "nightly-2026")
        self.git("tag", "cf-pages-abcdef")
        before = self.git("rev-parse", "HEAD")
        with patch.object(release.subprocess, "run", side_effect=fake_cliff) as run:
            result = release.draft(self.root, "0.1.0", "draft.md")
        command = run.call_args.args[0]
        self.assertEqual(command[0], "git-cliff")
        self.assertIsNone(result["base_tag"])
        self.assertFalse(any("..HEAD" in arg for arg in command))
        self.assertEqual(self.git("rev-parse", "HEAD"), before)
        self.assertEqual(self.git("tag", "--list").splitlines(), ["cf-pages-abcdef", "nightly-2026"])

    def test_next_draft_uses_semver_ancestor_not_latest_nightly(self):
        self.tag()
        self.write("next.txt", "next\n")
        self.commit("plain commit also needs notes")
        self.git("tag", "nightly-new")
        with patch.object(release.subprocess, "run", side_effect=fake_cliff) as run:
            result = release.draft(self.root, "0.2.0", "new.md")
        self.assertEqual(result["base_tag"], "v0.1.0")
        self.assertEqual(run.call_args.args[0][-1], "v0.1.0..HEAD")
        with self.assertRaises(ValueError):
            release.draft(self.root, "0.3.0", "other.md", "nightly-new")

    def test_draft_rc_boundary_is_explicitly_overridable(self):
        self.tag()
        self.write("next", "candidate")
        self.commit("candidate changes")
        self.git("tag", "v0.2.0-rc.1")
        self.write("next", "final")
        self.commit("final changes")
        with patch.object(release.subprocess, "run", side_effect=fake_cliff):
            nearest = release.draft(self.root, "0.2.0", "draft-nearest.md")
            stable = release.draft(self.root, "0.2.0", "draft-stable.md", "v0.1.0")
        self.assertEqual(nearest["base_tag"], "v0.2.0-rc.1")
        self.assertEqual(stable["base_tag"], "v0.1.0")

    def test_draft_refuses_existing_output_and_finalized_tag(self):
        with self.assertRaises(ValueError):
            release.draft(self.root, "0.2.0", "Cargo.toml")
        self.tag()
        with self.assertRaises(ValueError):
            release.draft(self.root, "0.1.0", "new.md")

    def test_config_does_not_discard_nonconventional_history(self):
        config = tomllib.loads((ROOT / "cliff.toml").read_text())
        for key in ("conventional_commits", "filter_unconventional", "require_conventional", "filter_commits"):
            self.assertFalse(config["git"][key])
        self.assertIn("commit.message", config["changelog"]["body"])

    def test_annotated_tag_pins_commit_and_tag_object(self):
        result = self.tag(annotated=True)
        self.assertEqual(result["commit"], self.git("rev-parse", "HEAD"))
        self.assertNotEqual(result["tag_object"], result["commit"])
        self.assertEqual(result["version"], "v0.1.0")
        self.git("tag", "-f", "-a", "v0.1.0", "-m", "different annotation")
        with self.assertRaisesRegex(ValueError, "tag object changed"):
            release.resolve(self.root, "v0.1.0", result["commit"], expected_tag_object=result["tag_object"])

    def test_moved_tag_and_mismatched_version_fail(self):
        result = self.tag()
        self.write("change", "changed")
        self.commit("Changed after release")
        self.git("tag", "-f", "v0.1.0")
        with self.assertRaisesRegex(ValueError, "immutable commit"):
            release.resolve(self.root, "v0.1.0", result["commit"])
        self.git("tag", "v0.2.0")
        with self.assertRaisesRegex(ValueError, "workspace version"):
            release.resolve(self.root, "v0.2.0")

    def test_notes_are_exact_committed_bytes_not_worktree_or_generator(self):
        result = self.tag()
        self.write("docs/releases/v0.1.0.md", "unreviewed worktree text\n")
        destination = self.root / "body.md"
        subprocess.run(["python3", str(ROOT / "tools/release/release.py"), "--repo", str(self.root),
                        "notes", "v0.1.0", "--expected-commit", result["commit"],
                        "--expected-tag-object", result["tag_object"], "--output", str(destination)],
                       check=True, capture_output=True)
        self.assertEqual(destination.read_bytes(), b"# v0.1.0\n\nReviewed release notes.\n")
        with self.assertRaises(ValueError):
            release.resolve(self.root, "v0.1.0", notes_path="../secret")

    def test_missing_placeholder_and_heading_only_notes_fail(self):
        for content in ("", "# v0.1.0\n", "# v0.1.0\nTODO: fill this\n", "- TODO: review\n",
                        "- [ ] TBD: fill\n", "> PLACEHOLDER: later\n", "<!-- TODO -->\n", "# DRAFT\ntext\n"):
            self.write("docs/releases/v0.1.0.md", content)
            self.commit("unfinalized notes")
            self.git("tag", "-f", "v0.1.0")
            with self.assertRaisesRegex(ValueError, "empty or contain"):
                release.resolve(self.root, "v0.1.0")
        with self.assertRaises(subprocess.CalledProcessError):
            release.resolve(self.root, "v0.1.0", notes_path="missing.md")

    def test_remote_tag_is_read_only_and_must_match_both_objects(self):
        result = self.tag(annotated=True)
        remote = Path(self.temporary.name) / "remote.git"
        subprocess.run(["git", "clone", "-q", "--bare", str(self.root), str(remote)], check=True)
        release.resolve(self.root, "v0.1.0", result["commit"], str(remote), expected_tag_object=result["tag_object"])
        subprocess.run(["git", "--git-dir", str(remote), "update-ref", "refs/tags/v0.1.0", result["commit"]], check=True)
        with self.assertRaisesRegex(ValueError, "remote release tag"):
            release.resolve(self.root, "v0.1.0", result["commit"], str(remote))

    def test_resolve_github_outputs_are_pinned(self):
        result = self.tag()
        output = self.root / "outputs"
        subprocess.run(["python3", str(ROOT / "tools/release/release.py"), "--repo", str(self.root),
                        "resolve", "v0.1.0", "--github-output", str(output)], check=True, capture_output=True)
        lines = dict(line.split("=", 1) for line in output.read_text().splitlines())
        self.assertEqual(lines, {key: result[key] for key in ("version", "commit", "tag_object")})

    def publication_fixture(self):
        result = self.tag(annotated=True)
        remote = Path(self.temporary.name) / "remote.git"
        subprocess.run(["git", "clone", "-q", "--bare", str(self.root), str(remote)], check=True)
        self.git("remote", "add", "origin", str(remote))
        assets = self.root / "out"
        assets.mkdir()
        for suffix in ("x86_64-linux-gnu-video.tar.gz", "x86_64-linux-gnu-video-source.tar.gz",
                       "aarch64-macos.zip", "x86_64-windows.zip", "site.zip", "web-sdk.zip"):
            (assets / ("animatix-v0.1.0-" + suffix)).write_bytes(b"fixture archive")
        return result, assets

    def test_publication_creates_once_from_committed_notes(self):
        result, assets = self.publication_fixture()
        self.write("docs/releases/v0.1.0.md", "not reviewed")
        calls = []

        def fake_gh(command, **kwargs):
            if command == ["git", "remote", "get-url", "origin"]:
                return subprocess.CompletedProcess(command, 0, stdout=b"git@github.com:j4ger/animatix.git\n")
            if command[0] != "gh":
                return REAL_RUN(command, **kwargs)
            calls.append(command)
            self.assertEqual(Path(command[command.index("--notes-file") + 1]).read_bytes(),
                             b"# v0.1.0\n\nReviewed release notes.\n")
            return subprocess.CompletedProcess(command, 0)

        with patch.object(release.subprocess, "run", side_effect=fake_gh), patch.dict(os.environ, {"GH_REPO": "wrong/repo"}):
            release.publish(self.root, "v0.1.0", result["commit"], result["tag_object"], "out", repository="j4ger/animatix")
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][:4], ["gh", "release", "create", "v0.1.0"])
        self.assertIn("--verify-tag", calls[0])
        self.assertEqual(calls[0][calls[0].index("--repo") + 1], "j4ger/animatix")
        self.assertEqual(calls[0][calls[0].index("--target") + 1], result["commit"])
        self.assertNotIn("--prerelease", calls[0])

    def test_existing_release_failure_never_retries_or_edits(self):
        result, assets = self.publication_fixture()
        calls = []

        def fake_gh(command, **kwargs):
            if command == ["git", "remote", "get-url", "origin"]:
                return subprocess.CompletedProcess(command, 0, stdout=b"git@github.com:j4ger/animatix.git\n")
            if command[0] != "gh":
                return REAL_RUN(command, **kwargs)
            calls.append(command)
            raise subprocess.CalledProcessError(1, command, stderr="already exists")

        with patch.object(release.subprocess, "run", side_effect=fake_gh):
            with self.assertRaises(subprocess.CalledProcessError):
                release.publish(self.root, "v0.1.0", result["commit"], result["tag_object"], assets, repository="j4ger/animatix")
        self.assertEqual(len(calls), 1)

    def test_publication_validation_fails_before_gh(self):
        result, assets = self.publication_fixture()
        calls = []

        def fake_gh(command, **kwargs):
            if command == ["git", "remote", "get-url", "origin"]:
                return subprocess.CompletedProcess(command, 0, stdout=b"https://github.com/j4ger/animatix.git\n")
            if command[0] == "gh":
                calls.append(command)
                raise AssertionError("publication must not run")
            return REAL_RUN(command, **kwargs)

        with patch.object(release.subprocess, "run", side_effect=fake_gh):
            with self.assertRaisesRegex(ValueError, "repository does not match"):
                release.publish(self.root, "v0.1.0", result["commit"], result["tag_object"], assets, repository="other/repo")
            for commit, oid in (("0" * 40, result["tag_object"]), (result["commit"], "0" * 40)):
                with self.assertRaises(ValueError):
                    release.publish(self.root, "v0.1.0", commit, oid, assets, repository="j4ger/animatix")
            (assets / "animatix-v0.1.0-site.zip").unlink()
            with self.assertRaises(ValueError):
                release.publish(self.root, "v0.1.0", result["commit"], result["tag_object"], assets, repository="j4ger/animatix")
        self.assertEqual(calls, [])

    def test_unfinalized_notes_and_version_mismatch_never_publish(self):
        self.write("docs/releases/v0.1.0.md", "TODO: review release notes\n")
        commit = self.commit("unreviewed notes")
        self.git("tag", "-a", "v0.1.0", "-m", "bad notes")
        result = {"commit": commit, "tag_object": self.git("rev-parse", "refs/tags/v0.1.0")}
        remote = Path(self.temporary.name) / "remote.git"
        subprocess.run(["git", "clone", "-q", "--bare", str(self.root), str(remote)], check=True)
        self.git("remote", "add", "origin", str(remote))
        assets = self.root / "out"
        assets.mkdir()
        (assets / "asset.zip").write_bytes(b"fixture")
        calls = []

        def fake_gh(command, **kwargs):
            if command == ["git", "remote", "get-url", "origin"]:
                return subprocess.CompletedProcess(command, 0, stdout=b"https://github.com/j4ger/animatix.git\n")
            if command[0] == "gh":
                calls.append(command)
                raise AssertionError("publication must not run")
            return REAL_RUN(command, **kwargs)

        with patch.object(release.subprocess, "run", side_effect=fake_gh):
            with self.assertRaisesRegex(ValueError, "placeholder"):
                release.publish(self.root, "v0.1.0", result["commit"], result["tag_object"], assets, repository="j4ger/animatix")
            # Version validation precedes notes or any GitHub mutation.
            self.git("tag", "v0.2.0")
            remote = Path(self.temporary.name) / "remote.git"
            subprocess.run(["git", "--git-dir", str(remote), "update-ref", "refs/tags/v0.2.0", result["commit"]], check=True)
            with self.assertRaisesRegex(ValueError, "workspace version"):
                release.publish(self.root, "v0.2.0", result["commit"], result["commit"], assets, repository="j4ger/animatix")
        self.assertEqual(calls, [])

    def prepare_bump(self):
        self.write("tools/release/bump-version.sh", (ROOT / "tools/release/bump-version.sh").read_text())
        self.write("tools/release/release.py", (ROOT / "tools/release/release.py").read_text())
        self.write("tools/ci/ci.sh", "#!/bin/sh\n[ \"${BUMP_MODE:-}\" != gate-fail ]\n")
        (self.root / "tools/ci/ci.sh").chmod(0o755)
        bin_dir = Path(self.temporary.name) / "bin"
        bin_dir.mkdir()
        cargo = bin_dir / "cargo"
        cargo.write_text("""#!/bin/sh
printf 'updated lock\\n' > Cargo.lock
case "$BUMP_MODE" in
  cargo-fail) exit 7 ;;
  interrupt) kill -INT "$PPID" ;;
  terminate) kill -TERM "$PPID" ;;
esac
exit 0
""")
        cargo.chmod(0o755)
        (self.root / "Cargo.toml").chmod(0o640)
        (self.root / "Cargo.lock").chmod(0o600)
        self.commit("release test tooling")
        return {**os.environ, "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"]}

    def test_bump_rolls_back_cargo_gate_failures_and_signals(self):
        env = self.prepare_bump()
        originals = {name: ((self.root / name).read_bytes(), (self.root / name).stat().st_mode & 0o777)
                     for name in ("Cargo.toml", "Cargo.lock")}
        head = self.git("rev-parse", "HEAD")
        self.write("unrelated.txt", "keep dirty work\n")
        for mode in ("cargo-fail", "gate-fail", "interrupt", "terminate"):
            with self.subTest(mode=mode):
                result = subprocess.run(["bash", "tools/release/bump-version.sh", "0.2.0"], cwd=self.root,
                                        env={**env, "BUMP_MODE": mode}, capture_output=True, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                for name, (payload, permissions) in originals.items():
                    self.assertEqual((self.root / name).read_bytes(), payload)
                    self.assertEqual((self.root / name).stat().st_mode & 0o777, permissions)
                self.assertEqual((self.root / "unrelated.txt").read_text(), "keep dirty work\n")
                self.assertEqual(self.git("rev-parse", "HEAD"), head)
                self.assertEqual(self.git("tag", "--list"), "")

    def test_bump_rejects_dirty_or_staged_lock_without_modifying_either(self):
        env = self.prepare_bump()
        manifest = (self.root / "Cargo.toml").read_bytes()
        self.write("Cargo.lock", "author changes\n")
        for staged in (False, True):
            if staged:
                self.git("add", "Cargo.lock")
            result = subprocess.run(["bash", "tools/release/bump-version.sh", "0.2.0"], cwd=self.root,
                                    env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b"staged or unstaged", result.stderr)
            self.assertEqual((self.root / "Cargo.toml").read_bytes(), manifest)
            self.assertEqual((self.root / "Cargo.lock").read_text(), "author changes\n")

    def test_bump_rejects_staged_change_even_when_worktree_matches_head(self):
        env = self.prepare_bump()
        original = (self.root / "Cargo.lock").read_bytes()
        self.write("Cargo.lock", "staged changes\n")
        self.git("add", "Cargo.lock")
        (self.root / "Cargo.lock").write_bytes(original)
        index_before = self.git("show", ":Cargo.lock")
        result = subprocess.run(["bash", "tools/release/bump-version.sh", "0.2.0"], cwd=self.root,
                                env=env, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.root / "Cargo.lock").read_bytes(), original)
        self.assertEqual(self.git("show", ":Cargo.lock"), index_before)

    def test_asset_contract_rejects_missing_extra_duplicate_arch_and_empty(self):
        _, assets = self.publication_fixture()
        self.assertEqual(len(release.release_assets(assets, "v0.1.0")), 6)
        extra = assets / "unexpected.zip"
        extra.write_bytes(b"extra")
        with self.assertRaises(ValueError):
            release.release_assets(assets, "v0.1.0")
        extra.unlink()
        duplicate = assets / "animatix-v0.1.0-x86_64-macos.zip"
        duplicate.write_bytes(b"second architecture")
        with self.assertRaises(ValueError):
            release.release_assets(assets, "v0.1.0")
        duplicate.unlink()
        target = assets / "animatix-v0.1.0-web-sdk.zip"
        target.write_bytes(b"")
        with self.assertRaises(ValueError):
            release.release_assets(assets, "v0.1.0")
        target.unlink()
        with self.assertRaises(ValueError):
            release.release_assets(assets, "v0.1.0")

    def test_failed_rollback_retains_original_backup(self):
        env = self.prepare_bump()
        backups = Path(self.temporary.name) / "backups"
        backups.mkdir()
        cp = Path(self.temporary.name) / "bin/cp"
        original_cp = shutil.which("cp")
        cp.write_text(f"""#!/bin/sh
case "$2" in */animatix-version.*/Cargo.toml) exit 9 ;; esac
exec '{original_cp}' "$@"
""")
        cp.chmod(0o755)
        original = (self.root / "Cargo.toml").read_bytes()
        result = subprocess.run(["bash", "tools/release/bump-version.sh", "0.2.0"], cwd=self.root,
                                env={**env, "BUMP_MODE": "cargo-fail", "TMPDIR": str(backups)},
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("rollback incomplete", result.stderr)
        retained = list(backups.glob("animatix-version.*"))
        self.assertEqual(len(retained), 1)
        self.assertEqual((retained[0] / "Cargo.toml").read_bytes(), original)

    def test_successful_bump_leaves_reviewable_files_without_commit_or_tag(self):
        env = self.prepare_bump()
        head = self.git("rev-parse", "HEAD")
        result = subprocess.run(["bash", "tools/release/bump-version.sh", "0.2.0"], cwd=self.root,
                                env={**env, "BUMP_MODE": "success"}, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('version = "0.2.0"', (self.root / "Cargo.toml").read_text())
        self.assertEqual((self.root / "Cargo.lock").read_text(), "updated lock\n")
        self.assertEqual(self.git("rev-parse", "HEAD"), head)
        self.assertEqual(self.git("tag", "--list"), "")
        self.assertEqual(self.git("diff", "--cached", "--name-only"), "")

    def test_bump_dry_run_needs_no_cog_and_preserves_files(self):
        self.write("tools/release/bump-version.sh", (ROOT / "tools/release/bump-version.sh").read_text())
        self.write("tools/release/release.py", (ROOT / "tools/release/release.py").read_text())
        before = (self.root / "Cargo.toml").read_bytes()
        result = subprocess.run(["bash", "tools/release/bump-version.sh", "--dry-run", "0.2.0"],
                                cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / "Cargo.toml").read_bytes(), before)
        for invalid in ("01.2.3", "0.2.0-bad..tag", "0.2.0oops"):
            self.assertNotEqual(subprocess.run(["bash", "tools/release/bump-version.sh", "--dry-run", invalid],
                                              cwd=self.root, capture_output=True).returncode, 0)


if __name__ == "__main__":
    unittest.main()
