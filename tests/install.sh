#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
mkdir -p "$test_dir/bin"

cat >"$test_dir/bin/uname" <<'EOF'
#!/bin/sh
printf '%s\n' unsupported
EOF

cat >"$test_dir/bin/cargo" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >"$INSTALL_TEST_CARGO_LOG"
EOF

chmod +x "$test_dir/bin/uname" "$test_dir/bin/cargo"

INSTALL_TEST_CARGO_LOG="$test_dir/cargo.log" \
GITGRAPH_VERSION=v9.8.7 \
PATH="$test_dir/bin:/usr/bin:/bin" \
  sh "$project_dir/install.sh" >/dev/null

actual=$(cat "$test_dir/cargo.log")
expected="install --git https://github.com/bjo4/gitgraph-tui --tag v9.8.7 --locked"
if [ "$actual" != "$expected" ]; then
  printf 'expected cargo args: %s\nactual cargo args:   %s\n' "$expected" "$actual" >&2
  exit 1
fi

printf '%s\n' "installer tests passed"
