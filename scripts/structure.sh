#!/usr/bin/env bash
# structure — direct ceilings on function length and file size (docs/ecs.md §8).
#
# This is the Phase 3 replacement for splitting files mechanically to satisfy
# the 800-line ratchet. It constrains the two things that actually hurt review
# and comprehension:
#
#   files      production <= 1000 lines, tests <= 2000 lines. Test paths are
#              */tests/*, */tests.rs and *_tests.rs. `crates/animatix-plugin-api/
#              src/lib.rs` is exempt (one repr(C) ABI surface). There is no
#              file allowlist: the ceiling is absolute.
#
#   functions  production <= 200 lines, test functions <= 300 lines. A
#              function's length is the line span from its `fn` line through
#              the closing brace, inclusive, so a multi-line signature, generics
#              and a `where` clause all count. A row in
#              scripts/function-size-allowlist.tsv freezes an over-limit
#              function at its current length:
#
#                  path<TAB>fn_name<TAB>max_lines
#
#              The actual length may not exceed `max_lines`, and a row must be
#              deleted when the function is split, shortened under its
#              threshold, renamed or moved (the check fails on a stale row).
#              A row binds every body of that name in the file; keep names
#              distinct if two same-named functions both need freezing.
#
#   mod.rs     a `mod.rs` over 100 lines that still declares `impl` or a
#              body-bearing `fn` is frozen in scripts/mod-rs-allowlist.tsv
#              (`path<TAB>max_lines`). Declarations-only mod.rs files are fine
#              at any length. Rows are removed when the file becomes
#              declaration-only or drops to <= 100 lines.
#
# Both allowlists are ratchets: delete rows, never add one for new code, and
# never raise a maximum. `--seed` refreshes the rows of the current tree but
# refuses to add a row or raise an existing maximum, so it cannot launder a new
# over-limit function or file into the baseline.
#
# Output:
#   FAIL path:line fn name: N lines > max M
#   FAIL stale row <allowlist>:<line>: ...
#   structure: ok (X files, Y functions, Z allowlisted)
#   (Z is the number of active ratchet rows: function rows plus frozen mod.rs.)
#
# Parser (embedded Perl — no python3/node dependency):
#   * comments (`//`, nested `/* */`), normal/byte strings, raw strings
#     (`r"…"`, `r#"…"#`, `br#"…"#`) and char literals are blanked before the
#     brace scan, so braces inside them cannot move a function boundary.
#   * `fn` may be `pub`, `async`, `const`, `unsafe`, `extern "C"`, generic or
#     multi-line; declarations ending in `;` (trait/extern prototypes) are not
#     counted, bodies in `impl`/`trait`/nested `fn` are.
#   * a test function is one in a test path, under a lexically detected
#     `#[cfg(test)] mod`, or carrying a `#[test]`-family attribute.
#   * limits: the signature scan tracks paren/bracket/angle depth and treats
#     the first depth-0 `{` as the body. A `{`/`}` or `<`/`>` inside a const
#     generic default or an unusual return type can confuse that heuristic;
#     the brace scan itself is exact. `fn` tokens emitted by macros are
#     counted as functions (no expansion is attempted).
#
# Self-test (`--self-test`; not part of CI):
#   creates an untracked 201-line `fn __structure_probe()` under
#   crates/animatix/src, asserts that `check` fails on it, asserts that `--seed`
#   refuses to add it to a copy of the allowlists, then removes it. The probe is
#   deliberately untracked: collection uses `git ls-files --others`, so an
#   untracked probe is seen.
#
# Usage:
#   scripts/structure.sh            # check (the `structure` gate)
#   scripts/structure.sh --list     # over-limit functions, longest first
#   scripts/structure.sh --seed     # refresh both allowlists (no adds/raises)
#   scripts/structure.sh --self-test
#   scripts/structure.sh --help
#
# Environment overrides (documentation/tests only; CI uses the defaults):
#   PROD_FILE_MAX (1000)  TEST_FILE_MAX (2000)
#   PROD_FN_MAX (200)     TEST_FN_MAX (300)      MOD_MAX (100)
# END USAGE
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

MODE=${1:-check}
usage() { echo "usage: scripts/structure.sh [check|--list|--seed|--self-test|--help]" >&2; }
case "$MODE" in
  check|--check|--list|--seed|--self-test) ;;
  -h|--help)
    sed -n '2,/^# END USAGE$/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
    exit 0
    ;;
  *)
    usage
    exit 2
    ;;
esac
[[ $# -le 1 ]] || { usage; exit 2; }

# Exercise the ratchet without touching the committed allowlists: a transient
# untracked over-limit probe must fail `check`, and `--seed` must refuse to add
# it. The baseline may itself be failing while a tree is mid-change, so the
# cleanup assertion compares against the pre-probe result rather than assuming
# a green tree.
self_test() {
  local script before_out before_rc probe_out probe_rc seed_out after_out after_rc
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  # Globals, not locals: the EXIT trap runs after this function's scope is gone.
  ST_PROBE="crates/animatix/src/__structure_probe_self_test_$$.rs"
  ST_TMPDIR="$(mktemp -d "${TMPDIR:-/tmp}/animatix-structure-selftest.XXXXXX")"
  trap 'rm -f -- "${ST_PROBE:-}"; rm -rf -- "${ST_TMPDIR:-}"' EXIT

  if before_out="$(bash "$script" check 2>&1)"; then before_rc=0; else before_rc=$?; fi

  {
    echo 'fn __structure_probe() {'
    local i
    for ((i = 2; i < 201; i++)); do
      echo "    let _ = $i;"
    done
    echo '}'
  } > "$ST_PROBE"
  echo "structure: self-test: created $ST_PROBE (201-line function)"

  if probe_out="$(bash "$script" check 2>&1)"; then probe_rc=0; else probe_rc=$?; fi
  if [ "$probe_rc" -eq 0 ]; then
    echo "self-test FAILED: check accepted an over-limit function" >&2
    printf '%s\n' "$probe_out" >&2
    return 1
  fi
  if ! grep -Fq "FAIL $ST_PROBE:1 fn __structure_probe: 201 lines > max 200" <<<"$probe_out"; then
    echo "self-test FAILED: check did not report the expected over-limit function" >&2
    printf '%s\n' "$probe_out" >&2
    return 1
  fi

  cp scripts/function-size-allowlist.tsv "$ST_TMPDIR/function-size-allowlist.tsv"
  cp scripts/mod-rs-allowlist.tsv "$ST_TMPDIR/mod-rs-allowlist.tsv"
  if seed_out="$(STRUCTURE_ALLOWLIST="$ST_TMPDIR/function-size-allowlist.tsv" \
                  STRUCTURE_MOD_ALLOWLIST="$ST_TMPDIR/mod-rs-allowlist.tsv" \
                  bash "$script" --seed 2>&1)"; then
    echo "self-test FAILED: --seed accepted a new over-limit function" >&2
    printf '%s\n' "$seed_out" >&2
    return 1
  fi
  if ! grep -Fq "FAIL refusing to add $ST_PROBE __structure_probe" <<<"$seed_out"; then
    echo "self-test FAILED: --seed did not refuse to add the new over-limit function" >&2
    printf '%s\n' "$seed_out" >&2
    return 1
  fi
  cmp -s "$ST_TMPDIR/function-size-allowlist.tsv" scripts/function-size-allowlist.tsv || {
    echo "self-test FAILED: --seed modified the function allowlist before refusing" >&2
    return 1
  }
  cmp -s "$ST_TMPDIR/mod-rs-allowlist.tsv" scripts/mod-rs-allowlist.tsv || {
    echo "self-test FAILED: --seed modified the mod.rs allowlist before refusing" >&2
    return 1
  }

  rm -f -- "$ST_PROBE"
  if after_out="$(bash "$script" check 2>&1)"; then after_rc=0; else after_rc=$?; fi
  if [ "$after_rc" -ne "$before_rc" ] || [ "$after_out" != "$before_out" ]; then
    echo "self-test FAILED: removing the probe did not restore the pre-test result" >&2
    return 1
  fi
  echo "structure: self-test: ok (check and --seed both reject a new over-limit function)"
  return 0
}

if [ "$MODE" = "--self-test" ]; then
  self_test
  exit $?
fi

# Include untracked-but-not-ignored files: a new source file must be gated
# before it is staged, and the self-test probe is deliberately untracked.
# `vendor/**` is third-party source kept verbatim (e.g. a patched crate); it
# does not follow this repository's file/function structure contract.
collect() {
  git ls-files -z --cached --others --exclude-standard -- '*.rs' ':(exclude)vendor/**'
}

files=()
while IFS= read -r -d '' f; do
  files+=("$f")
done < <(collect)

if (( ${#files[@]} == 0 )); then
  echo "error: no Rust files found (run from the repository root)" >&2
  exit 2
fi

STRUCTURE_MODE="$MODE" \
PROD_FILE_MAX="${PROD_FILE_MAX:-1000}" \
TEST_FILE_MAX="${TEST_FILE_MAX:-2000}" \
PROD_FN_MAX="${PROD_FN_MAX:-200}" \
TEST_FN_MAX="${TEST_FN_MAX:-300}" \
MOD_MAX="${MOD_MAX:-100}" \
STRUCTURE_ALLOWLIST="${STRUCTURE_ALLOWLIST:-scripts/function-size-allowlist.tsv}" \
STRUCTURE_MOD_ALLOWLIST="${STRUCTURE_MOD_ALLOWLIST:-scripts/mod-rs-allowlist.tsv}" \
perl - "${files[@]}" <<'PERL'
use strict;
use warnings;
use File::Basename qw(basename);

my $MODE           = $ENV{STRUCTURE_MODE} || 'check';
my $PROD_FILE_MAX  = $ENV{PROD_FILE_MAX};
my $TEST_FILE_MAX  = $ENV{TEST_FILE_MAX};
my $PROD_FN_MAX    = $ENV{PROD_FN_MAX};
my $TEST_FN_MAX    = $ENV{TEST_FN_MAX};
my $MOD_MAX        = $ENV{MOD_MAX};
my $ALLOWLIST      = $ENV{STRUCTURE_ALLOWLIST};
my $MOD_ALLOWLIST  = $ENV{STRUCTURE_MOD_ALLOWLIST};
my @FILES          = @ARGV;

for my $pair (['PROD_FILE_MAX', $PROD_FILE_MAX], ['TEST_FILE_MAX', $TEST_FILE_MAX],
              ['PROD_FN_MAX', $PROD_FN_MAX], ['TEST_FN_MAX', $TEST_FN_MAX],
              ['MOD_MAX', $MOD_MAX]) {
  unless (defined $pair->[1] && $pair->[1] =~ /^[0-9]+$/) {
    print STDERR "error: $pair->[0] must be a non-negative integer, got '", ($pair->[1] // ''), "'\n";
    exit 2;
  }
}

my $FAIL = 0;
sub fail {
  print STDERR @_, "\n";
  $FAIL = 1;
}

sub fatal {
  print STDERR @_, "\n";
  exit 2;
}

sub is_test_path {
  my $p = shift;
  return 1 if $p =~ m{(?:^|/)tests/};
  return 1 if $p =~ m{(?:^|/)tests\.rs$};
  return 1 if $p =~ m{_tests\.rs$};
  return 0;
}

sub is_file_exempt {
  # The native plugin ABI is one repr(C) surface; docs/ecs.md §8 keeps it as
  # the single source exemption. Its *functions* are still length-checked.
  return $_[0] eq 'crates/animatix-plugin-api/src/lib.rs';
}

sub blank {
  my $s = shift;
  $s =~ s/[^\n]/ /g;
  return $s;
}

# Replace comments and literals with spaces, preserving newlines and byte
# offsets, so line numbers and brace/`fn` scanning operate on code only.
sub clean_source {
  my ($src) = @_;
  my $n = length $src;
  my $out = '';
  my $i = 0;
  while ($i < $n) {
    # Fast path: copy runs that cannot start a comment or literal. `r` is
    # kept special because it may start a raw string.
    pos($src) = $i;
    if ($src =~ /\G([^\/"'r]+)/gc) {
      $out .= $1;
      $i = pos($src);
      next;
    }
    my $c  = substr($src, $i, 1);
    my $c2 = substr($src, $i + 1, 1);

    if ($c eq '/' && $c2 eq '/') {
      my $j = index($src, "\n", $i);
      $j = $n if $j < 0;
      $out .= blank(substr($src, $i, $j - $i));
      $i = $j;
      next;
    }

    if ($c eq '/' && $c2 eq '*') {
      my $j = $i + 2;
      my $depth = 1;
      while ($depth > 0) {
        my $open  = index($src, '/*', $j);
        my $close = index($src, '*/', $j);
        if ($close < 0) { $j = $n; last; }          # unterminated
        if ($open >= 0 && $open < $close) { $depth++; $j = $open + 2; }
        else { $depth--; $j = $close + 2; }
      }
      $out .= blank(substr($src, $i, $j - $i));
      $i = $j;
      next;
    }

    if ($c eq '"') {
      my $j = $i + 1;
      while ($j < $n) {
        my $d = substr($src, $j, 1);
        if ($d eq '\\') { $j += 2; next; }
        if ($d eq '"') { $j++; last; }
        $j++;
      }
      $out .= blank(substr($src, $i, $j - $i));
      $i = $j;
      next;
    }

    if ($c eq "'") {
      # A real char literal, not a lifetime. `'a'`, `'\n'`, `'\u{1F600}'`,
      # `'\''`. Lifetimes fall through and stay code.
      my $head = substr($src, $i, 16);
      if ($head =~ /\A'(?:\\(?:[nrt0\\'"]|x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]{1,6}\})|[^\\'\n])'/) {
        my $len = length $&;
        $out .= blank(substr($src, $i, $len));
        $i += $len;
        next;
      }
      $out .= $c;
      $i++;
      next;
    }

    if ($c eq 'r') {
      my $prev = $i > 0 ? substr($src, $i - 1, 1) : '';
      my $rewind = 0;
      my $ok = 1;
      if ($prev =~ /[A-Za-z0-9_]/) {
        # `br"…"` / `cr"…"`: the raw marker is the `r`, the prefix is before it.
        if (($prev eq 'b' || $prev eq 'c') &&
            ($i < 2 || substr($src, $i - 2, 1) !~ /[A-Za-z0-9_]/)) {
          $rewind = 1;
        } else {
          $ok = 0;
        }
      }
      if ($ok) {
        my $head = substr($src, $i, 512);
        if ($head =~ /\Ar(#*)(?=")/) {
          my $hashes = length $1;
          my $start = $i - $rewind;
          my $open_end = $i + 1 + $hashes + 1;
          my $close = '"' . ('#' x $hashes);
          my $endq = index($src, $close, $open_end);
          my $end = $endq < 0 ? $n : $endq + length($close);
          $out = substr($out, 0, length($out) - $rewind) if $rewind;
          $out .= blank(substr($src, $start, $end - $start));
          $i = $end;
          next;
        }
      }
      $out .= 'r';
      $i++;
      next;
    }

    $out .= $c;
    $i++;
  }
  return $out;
}

sub line_starts {
  my ($src) = @_;
  my @starts = (0);
  while ($src =~ /\n/g) {
    push @starts, pos($src);
  }
  return \@starts;
}

sub line_of {
  my ($starts, $pos) = @_;
  my ($lo, $hi) = (0, $#$starts);
  while ($lo < $hi) {
    my $mid = int(($lo + $hi + 1) / 2);
    if ($starts->[$mid] <= $pos) { $lo = $mid; } else { $hi = $mid - 1; }
  }
  return $lo + 1;
}

sub match_brace {
  my ($s, $p) = @_;
  my $n = length $s;
  my $depth = 0;
  pos($s) = $p;
  while ($s =~ /\G[^{}]*([{}])/gc) {
    if ($1 eq '{') {
      $depth++;
    } else {
      $depth--;
      return pos($s) - 1 if $depth == 0;
    }
  }
  return $n - 1;    # unbalanced input; parse nothing further for this item
}

# From just after the function name, find the opening brace of the body.
# Returns () for a declaration (`fn f();`), ($open, $close) for a body.
sub find_body {
  my ($s, $p) = @_;
  my $n = length $s;
  my ($par, $brk, $ang) = (0, 0, 0);
  while ($p < $n) {
    my $c = substr($s, $p, 1);
    if    ($c eq '(') { $par++ }
    elsif ($c eq ')') { $par-- if $par > 0 }
    elsif ($c eq '[') { $brk++ }
    elsif ($c eq ']') { $brk-- if $brk > 0 }
    elsif ($c eq '<') { $ang++ }
    elsif ($c eq '>') { $ang-- if $ang > 0 }
    elsif ($c eq '{') {
      return ($p, match_brace($s, $p)) if $par == 0 && $brk == 0 && $ang == 0;
    }
    elsif (($c eq ';' || $c eq '}') && $par == 0 && $brk == 0 && $ang == 0) {
      return;
    }
    $p++;
  }
  return;
}

# Ranges of `#[cfg(test)] mod … { … }` bodies, so helpers in inline test
# modules get the test threshold without an attribute on every function.
sub test_regions {
  my ($s) = @_;
  my @regions;
  while ($s =~ /#\[\s*cfg\s*\([^]]*\btest\b[^]]*\)\s*\]/g) {
    my $after = pos($s);
    my $rest = substr($s, $after, 2048);
    if ($rest =~ /\A(?:\s*#\[[^\]]*\])*\s*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+[A-Za-z_][A-Za-z0-9_]*\s*\{/) {
      my $brace = $after + $-[0] + index($&, '{');
      push @regions, [$brace, match_brace($s, $brace)];
    }
  }
  return \@regions;
}

# Attribute blocks immediately preceding a position, outermost first.
sub attrs_before {
  my ($s, $pos) = @_;
  my @attrs;
  my $p = $pos;
  while ($p > 0) {
    my $q = $p;
    $q-- while $q > 0 && substr($s, $q - 1, 1) =~ /\s/;
    return @attrs if $q == 0;
    return @attrs unless substr($s, $q - 1, 1) eq ']';
    my $d = 0;
    my $k = $q - 1;
    while ($k >= 0) {
      my $c = substr($s, $k, 1);
      if ($c eq ']') { $d++; }
      elsif ($c eq '[') { $d--; last if $d == 0; }
      $k--;
    }
    return @attrs if $k < 0;
    my $h = $k - 1;
    $h-- while $h >= 0 && substr($s, $h, 1) =~ /\s/;
    return @attrs unless $h >= 0 && substr($s, $h, 1) eq '#';
    unshift @attrs, substr($s, $h, $q - $h);
    $p = $h;
  }
  return @attrs;
}

sub has_test_attr {
  my ($attrs) = @_;
  for my $a (@$attrs) {
    return 1 if $a =~ /#\[\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*test\s*(?:\(|\])/;
    return 1 if $a =~ /#\[\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*(?:rstest|test_case|proptest)\b/;
    return 1 if $a =~ /#\[\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*cfg\s*\([^]]*\btest\b/;
  }
  return 0;
}

sub parse_file {
  my ($src) = @_;
  my $clean = clean_source($src);
  my $starts = line_starts($src);
  my $regions = test_regions($clean);
  my @fns;

  while ($clean =~ /(?<![A-Za-z0-9_])fn\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)/g) {
    my $fn_pos = $-[0];
    my $name = $1;
    my $sig_end = $+[0];
    my ($body_start, $body_end) = find_body($clean, $sig_end);
    next unless defined $body_start;

    my $line = line_of($starts, $fn_pos);
    my $end_line = line_of($starts, $body_end);
    my $len = $end_line - $line + 1;

    my $test = 0;
    for my $r (@$regions) {
      if ($fn_pos > $r->[0] && $fn_pos < $r->[1]) { $test = 1; last; }
    }
    if (!$test) {
      my @attrs = attrs_before($clean, $fn_pos);
      $test = has_test_attr(\@attrs);
    }

    push @fns, { name => $name, line => $line, len => $len, test => $test };
  }

  return {
    fns      => \@fns,
    nlines   => scalar(@$starts) - 1,    # same as `wc -l`
    has_impl => ($clean =~ /\bimpl\b/) ? 1 : 0,
  };
}

sub slurp {
  my ($path) = @_;
  open my $fh, '<:raw', $path or return undef;
  local $/;
  my $s = <$fh>;
  close $fh;
  return $s;
}

sub load_rows {
  my ($file, $kind) = @_;
  my (%rows, %lines);
  return (\%rows, \%lines) unless -f $file;
  open my $fh, '<', $file or fatal "error: cannot read $file: $!";
  my $lineno = 0;
  while (my $l = <$fh>) {
    $lineno++;
    $l =~ s/\r?\n\z//;
    next if $l =~ /^\s*$/ || $l =~ /^#/;
    my @f = split /\t/, $l, -1;
    my ($path, $name, $max);
    if ($kind eq 'fn') {
      fatal "error: $file:$lineno: expected path<TAB>fn_name<TAB>max_lines" unless @f == 3;
      ($path, $name, $max) = @f;
      fatal "error: $file:$lineno: empty path or function name" if $path eq '' || $name eq '';
    } else {
      fatal "error: $file:$lineno: expected path<TAB>max_lines" unless @f == 2;
      ($path, $max) = @f;
      $name = '';
      fatal "error: $file:$lineno: empty path" if $path eq '';
    }
    fatal "error: $file:$lineno: max_lines must be a positive integer, got '$max'"
      unless $max =~ /^[0-9]+$/ && $max > 0;
    my $key = $kind eq 'fn' ? "$path\t$name" : $path;
    fatal "error: $file:$lineno: duplicate entry for ", ($kind eq 'fn' ? "$path $name" : $path)
      if exists $rows{$key};
    $rows{$key} = $max;
    $lines{$key} = $lineno;
  }
  close $fh;
  return (\%rows, \%lines);
}

sub write_rows {
  my ($file, $text) = @_;
  my $tmp = "$file.tmp.$$";
  open my $fh, '>', $tmp or fatal "error: cannot write $tmp: $!";
  print {$fh} $text or fatal "error: cannot write $tmp: $!";
  close $fh or fatal "error: cannot write $tmp: $!";
  rename $tmp, $file or fatal "error: cannot replace $file: $!";
}

# ── scan the tree ────────────────────────────────────────────────────────────
my %files;        # path -> { fns, nlines, has_impl }
my %groups;       # "path\tname" -> [ fn, … ] (same-named functions in a file)
my $total_fns = 0;

for my $path (sort @FILES) {
  next unless -f $path;
  my $src = slurp($path);
  unless (defined $src) {
    fail "FAIL $path: cannot read file";
    next;
  }
  my $info = parse_file($src);
  if (is_test_path($path)) {
    $_->{test} = 1 for @{ $info->{fns} };
  }
  $files{$path} = $info;
  for my $fn (@{ $info->{fns} }) {
    push @{ $groups{"$path\t$fn->{name}"} }, $fn;
    $total_fns++;
  }
}

if (!@FILES) {
  print STDERR "error: no Rust files found (run from the repository root)\n";
  exit 2;
}

# ── seed ─────────────────────────────────────────────────────────────────────
if ($MODE eq '--seed') {
  my ($old_fn, $old_fn_line) = load_rows($ALLOWLIST, 'fn');
  my ($old_mod, $old_mod_line) = load_rows($MOD_ALLOWLIST, 'mod');

  my %want_fn;
  for my $key (sort keys %groups) {
    my $cap = 0;
    my $needed = 0;
    for my $fn (@{ $groups{$key} }) {
      my $limit = $fn->{test} ? $TEST_FN_MAX : $PROD_FN_MAX;
      if ($fn->{len} > $limit) {
        $needed = 1;
        $cap = $fn->{len} if $fn->{len} > $cap;
      }
    }
    $want_fn{$key} = $cap if $needed;
  }

  my %want_mod;
  for my $path (sort keys %files) {
    next unless basename($path) eq 'mod.rs';
    my $info = $files{$path};
    next unless $info->{nlines} > $MOD_MAX;
    next unless $info->{has_impl} || @{ $info->{fns} };
    $want_mod{$path} = $info->{nlines};
  }

  my @raises;
  for my $key (sort keys %want_fn) {
    my ($path, $name) = split /\t/, $key, 2;
    unless (exists $old_fn->{$key}) {
      # A row for new code is how `--seed` used to launder an over-limit
      # function into the baseline. The allowlists are a ratchet: a new
      # offender must be split, never frozen.
      push @raises, "FAIL refusing to add $path $name (new over-limit function; split it instead)";
      next;
    }
    if ($want_fn{$key} > $old_fn->{$key}) {
      push @raises, "FAIL refusing to raise $path $name from $old_fn->{$key} to $want_fn{$key} (split it instead)";
    }
  }
  for my $path (sort keys %want_mod) {
    unless (exists $old_mod->{$path}) {
      push @raises, "FAIL refusing to add $path (new over-limit mod.rs; split it instead)";
      next;
    }
    if ($want_mod{$path} > $old_mod->{$path}) {
      push @raises, "FAIL refusing to raise $path from $old_mod->{$path} to $want_mod{$path} (split it instead)";
    }
  }
  if (@raises) {
    print STDERR "$_\n" for @raises;
    print STDERR "structure: seed aborted; both allowlists are a ratchet (docs/ecs.md §8)\n";
    exit 1;
  }

  my $fn_text = <<"HEADER";
# path<TAB>fn_name<TAB>max_lines
# Frozen at the Phase 3 baseline. This is a ratchet: delete a row when the
# function is split, shortened below its threshold, renamed or moved; never add
# a row for new code and never raise an existing maximum. See docs/ecs.md §8.
HEADER
  $fn_text .= "$_\t$want_fn{$_}\n" for sort keys %want_fn;

  my $mod_text = <<"HEADER";
# path<TAB>max_lines
# Frozen at the Phase 3 baseline: mod.rs is over 100 lines and still declares
# impl/fn bodies. Ratchet: delete a row when the file becomes declaration-only
# or drops to <= 100 lines; never add or raise one. See docs/ecs.md §8.
HEADER
  $mod_text .= "$_\t$want_mod{$_}\n" for sort keys %want_mod;

  write_rows($ALLOWLIST, $fn_text);
  write_rows($MOD_ALLOWLIST, $mod_text);
  print "structure: wrote $ALLOWLIST (", scalar(keys %want_fn), " entries) and ",
        "$MOD_ALLOWLIST (", scalar(keys %want_mod), " entries)\n";

  ($old_fn, $old_fn_line) = load_rows($ALLOWLIST, 'fn');
  ($old_mod, $old_mod_line) = load_rows($MOD_ALLOWLIST, 'mod');
}

# ── list ─────────────────────────────────────────────────────────────────────
if ($MODE eq '--list') {
  my ($lf, $lm) = load_rows($ALLOWLIST, 'fn');
  my @rows;
  for my $key (sort keys %groups) {
    my ($path, $name) = split /\t/, $key, 2;
    for my $fn (@{ $groups{$key} }) {
      my $limit = $fn->{test} ? $TEST_FN_MAX : $PROD_FN_MAX;
      next unless $fn->{len} > $limit;
      my $frozen = exists $lf->{$key} ? " [allowlisted max $lf->{$key}]" : '';
      push @rows, [$fn->{len}, $fn->{test} ? 'test' : 'source',
                   "$path:$fn->{line}", $name, $frozen];
    }
  }
  for my $r (sort { $b->[0] <=> $a->[0] || $a->[2] cmp $b->[2] } @rows) {
    printf "%6d  %-6s %s fn %s%s\n", $r->[0], $r->[1], $r->[2], $r->[3], $r->[4];
  }
  print scalar(@rows), " function(s) over their limit\n";
  exit 0;
}

# ── check ────────────────────────────────────────────────────────────────────
my ($old_fn, $old_fn_line) = load_rows($ALLOWLIST, 'fn');
my ($old_mod, $old_mod_line) = load_rows($MOD_ALLOWLIST, 'mod');

my %active_fn;
my %active_mod;

# File ceilings.
for my $path (sort keys %files) {
  next if is_file_exempt($path);
  my $info = $files{$path};
  my $max = is_test_path($path) ? $TEST_FILE_MAX : $PROD_FILE_MAX;
  next unless $info->{nlines} > $max;
  my $kind = is_test_path($path) ? 'test' : 'production';
  fail "FAIL $path: $info->{nlines} lines > max $max ($kind file; split it)";
}

# Function ceilings.
for my $key (sort keys %groups) {
  my ($path, $name) = split /\t/, $key, 2;
  for my $fn (sort { $a->{line} <=> $b->{line} } @{ $groups{$key} }) {
    my $limit = $fn->{test} ? $TEST_FN_MAX : $PROD_FN_MAX;
    next unless $fn->{len} > $limit;
    my $kind = $fn->{test} ? 'test' : 'production';
    if (exists $old_fn->{$key}) {
      $active_fn{$key} = 1;
      if ($fn->{len} > $old_fn->{$key}) {
        fail "FAIL $path:$fn->{line} fn $name: $fn->{len} lines > max $old_fn->{$key} "
           . "(allowlist frozen at $old_fn->{$key}; $kind limit $limit)";
      }
    } else {
      fail "FAIL $path:$fn->{line} fn $name: $fn->{len} lines > max $limit "
         . "($kind function; split it, or freeze it in $ALLOWLIST)";
    }
  }
}

# Stale function rows: the function must still exist and still be over limit.
for my $key (sort keys %$old_fn) {
  my ($path, $name) = split /\t/, $key, 2;
  my $lineno = $old_fn_line->{$key};
  if (!exists $files{$path}) {
    fail "FAIL stale row $ALLOWLIST:$lineno: $path $name — file is gone; delete the row";
    next;
  }
  my $fns = $groups{$key};
  unless ($fns) {
    fail "FAIL stale row $ALLOWLIST:$lineno: $path $name — no function with that name remains; delete the row";
    next;
  }
  my $over = 0;
  my $longest = 0;
  for my $fn (@$fns) {
    my $limit = $fn->{test} ? $TEST_FN_MAX : $PROD_FN_MAX;
    $longest = $fn->{len} if $fn->{len} > $longest;
    $over = 1 if $fn->{len} > $limit;
  }
  unless ($over) {
    fail "FAIL stale row $ALLOWLIST:$lineno: $path $name — now $longest lines "
       . "(at or under its threshold); delete the row";
  }
}

# mod.rs: implementations still in a mod.rs that is over 100 lines.
for my $path (sort keys %files) {
  next unless basename($path) eq 'mod.rs';
  my $info = $files{$path};
  next unless $info->{nlines} > $MOD_MAX;
  next unless $info->{has_impl} || @{ $info->{fns} };
  if (exists $old_mod->{$path}) {
    $active_mod{$path} = 1;
    if ($info->{nlines} > $old_mod->{$path}) {
      fail "FAIL $path: $info->{nlines} lines > max $old_mod->{$path} "
         . "(mod.rs allowlist frozen at $old_mod->{$path}; still > $MOD_MAX and holds impl/fn)";
    }
  } else {
    fail "FAIL $path: $info->{nlines} lines > max $MOD_MAX "
       . "(mod.rs holds impl/fn; move implementations into a domain module, or freeze it in $MOD_ALLOWLIST)";
  }
}

for my $path (sort keys %$old_mod) {
  my $lineno = $old_mod_line->{$path};
  unless (basename($path) eq 'mod.rs') {
    fail "FAIL stale row $MOD_ALLOWLIST:$lineno: $path — not a mod.rs; delete the row";
    next;
  }
  unless (exists $files{$path}) {
    fail "FAIL stale row $MOD_ALLOWLIST:$lineno: $path — file is gone; delete the row";
    next;
  }
  my $info = $files{$path};
  my $qualifies = $info->{nlines} > $MOD_MAX && ($info->{has_impl} || @{ $info->{fns} });
  unless ($qualifies) {
    fail "FAIL stale row $MOD_ALLOWLIST:$lineno: $path — no longer a > $MOD_MAX mod.rs "
       . "with impl/fn; delete the row";
  }
}

# ── summary ──────────────────────────────────────────────────────────────────
if ($FAIL) {
  exit 1;
}
print "structure: ok (", scalar(keys %files), " files, $total_fns functions, ",
      scalar(keys %active_fn) + scalar(keys %active_mod), " allowlisted)\n";
exit 0;
PERL
