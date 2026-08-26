PREFIX ?= ~/.local/bin
# man finds ~/.local/share/man automatically when ~/.local/bin is on PATH.
MANDIR ?= ~/.local/share/man/man1
# bash-completion's dynamic loader searches $XDG_DATA_HOME (user) and
# $XDG_DATA_DIRS (which includes /usr/local/share) for
# bash-completion/completions/<command>.
COMPDIR ?= ~/.local/share/bash-completion/completions
# Claude Code assets (skills + hooks) ship beside the script so that
# `sinteractive --install-claude` works from an installed copy and not only
# from a git checkout. Mirrors the repo layout, so the lookup in the script
# has one code path for both.
SHAREDIR ?= ~/.local/share/sinteractive

# Every skill under skills/ ships, discovered rather than listed: adding one
# is a matter of dropping a directory in, with no install target to update.
# The paths are relative and mirror the repo layout, so `install -D` into a
# destination root reproduces skills/<name>/SKILL.md underneath it.
SKILLS := $(wildcard skills/*/SKILL.md)

# When run as root, install system-wide: sinteractive to /usr/local/bin and
# its man page to /usr/local/share/man.
UID := $(shell id -u)

.PHONY: install install-user install-system claude-install skill-install

ifeq ($(UID),0)
install: install-system
else
install: install-user
endif

install-user:
	mkdir -p $(PREFIX)
	cp sinteractive $(PREFIX)/sinteractive
	chmod +x $(PREFIX)/sinteractive
	mkdir -p $(MANDIR)
	cp man/sinteractive.1 $(MANDIR)/sinteractive.1
	mkdir -p $(COMPDIR)
	cp completions/sinteractive.bash $(COMPDIR)/sinteractive
	mkdir -p $(SHAREDIR)/claude/hooks
	install -m 0755 claude/hooks/*.sh $(SHAREDIR)/claude/hooks/
	install -m 0644 claude/settings-snippet.json $(SHAREDIR)/claude/
	for s in $(SKILLS); do install -D -m 0644 $$s $(SHAREDIR)/$$s; done

install-system:
	install -m 0755 sinteractive /usr/local/bin/sinteractive
	install -D -m 0644 man/sinteractive.1 /usr/local/share/man/man1/sinteractive.1
	install -D -m 0644 completions/sinteractive.bash /usr/local/share/bash-completion/completions/sinteractive
	install -d -m 0755 /usr/local/share/sinteractive/claude/hooks
	install -m 0755 claude/hooks/*.sh /usr/local/share/sinteractive/claude/hooks/
	install -m 0644 claude/settings-snippet.json /usr/local/share/sinteractive/claude/
	for s in $(SKILLS); do install -D -m 0644 $$s /usr/local/share/sinteractive/$$s; done

# ---------------------------------------------------------------------------
# Claude Code integration. Two parts:
#
#   - skills, which teach an agent how work is done here, loaded on demand
#     from their descriptions rather than all at once:
#
#       bodhi-compute    cluster etiquette; a session is not a compute target
#       slurm-discovery  partitions, accounts, QOS, and what you may submit
#       bodhi-storage    /beevol vs node-local /tmp, and where output belongs
#       bodhi-software   modules first, then containers, then pixi/uv
#       slurm-batch      sbatch, arrays, dependencies, sizing from sacct
#       git-workflow     semver, Conventional Commits, worktrees, PRs
#
#     The first five are about the cluster; git-workflow is about the
#     repository open in the session;
#   - two hooks for an agent running INSIDE a session, which tell it at
#     startup where it is and how big the allocation is, and warn it when the
#     session is running out of walltime.
#
# All per-user, so this installs into ~/.claude regardless of UID.
#
# Registration is left to the script: sinteractive --install-claude merges the
# hooks into settings.json with jq, or prints the block when there is no jq.
# Either way the merge logic lives in one place rather than being mirrored
# here.
# ---------------------------------------------------------------------------
CLAUDE_DIR ?= $(HOME)/.claude

claude-install:
	@CLAUDE_CONFIG_DIR=$(CLAUDE_DIR) SINTERACTIVE_SHARE=$(CURDIR) ./sinteractive --install-claude

# Back-compat alias: this target used to install only the skill.
skill-install: claude-install

# ---------------------------------------------------------------------------
# tmux — build the latest release from source and install to $(TMUX_PREFIX).
#
# sinteractive runs $(TMUX_PREFIX)/bin/tmux ON THE ALLOCATED COMPUTE NODE, and
# /usr/local is node-local (root fs, not shared), so the binary must exist on
# every compute node. Build once with `make tmux`, then fan it out with
# `make tmux-push`.
#
# Bump the version here (or `make tmux TMUX_VERSION=3.8`) — see the release
# list at https://github.com/tmux/tmux/wiki
# ---------------------------------------------------------------------------
TMUX_VERSION     ?= 3.7c
TMUX_PREFIX      ?= /usr/local
TMUX_URL          = https://github.com/tmux/tmux/releases/download/$(TMUX_VERSION)/tmux-$(TMUX_VERSION).tar.gz
TMUX_BUILD_DIR   ?= /tmp/tmux-build-$(TMUX_VERSION)
CONFIGURE_FLAGS  ?=

# Compute nodes to push the built binary to (this head/login node builds it).
# Defaults to every node Slurm knows about; override with `make tmux-push NODES="compute00 compute01"`.
NODES            ?= $(shell sinfo -hN -o '%N' 2>/dev/null | sort -u)
SSH_USER         ?= root

.PHONY: tmux-deps tmux tmux-push tmux-all nodes nodes-check require-root

# The tmux targets install system-wide (into $(TMUX_PREFIX)) and push to other
# nodes, so they must be run as root.
require-root:
	@test "$(UID)" = "0" || { echo "error: tmux targets must be run as root"; exit 1; }

# Build dependencies (RHEL/Rocky 9). Run once per node that compiles tmux.
tmux-deps: require-root
	dnf install -y gcc make bison libevent-devel ncurses-devel

# Download, configure, build, and install into $(TMUX_PREFIX).
tmux: require-root
	@test -f /usr/include/event2/event.h || { \
	  echo "libevent-devel headers missing — run 'make tmux-deps' first"; exit 1; }
	rm -rf $(TMUX_BUILD_DIR) && mkdir -p $(TMUX_BUILD_DIR)
	curl -LfsS $(TMUX_URL) | tar xz -C $(TMUX_BUILD_DIR) --strip-components=1
	cd $(TMUX_BUILD_DIR) && ./configure --prefix=$(TMUX_PREFIX) $(CONFIGURE_FLAGS)
	$(MAKE) -C $(TMUX_BUILD_DIR) -j$(shell nproc)
	$(MAKE) -C $(TMUX_BUILD_DIR) install
	rm -rf $(TMUX_BUILD_DIR)
	@$(TMUX_PREFIX)/bin/tmux -V

# Fan the freshly built binary out to the compute nodes. Copies to a temp name
# and renames into place so running sinteractive sessions aren't disturbed
# ("text file busy" / clobbering a live server's inode).
tmux-push: require-root
	@test -x $(TMUX_PREFIX)/bin/tmux || { echo "build first: make tmux"; exit 1; }
	@for n in $(NODES); do \
	  printf '==> %s: ' "$$n"; \
	  scp -q $(TMUX_PREFIX)/bin/tmux $(SSH_USER)@$$n:$(TMUX_PREFIX)/bin/tmux.new \
	    && ssh $(SSH_USER)@$$n \
	      'install -m 0755 $(TMUX_PREFIX)/bin/tmux.new $(TMUX_PREFIX)/bin/tmux \
	       && rm -f $(TMUX_PREFIX)/bin/tmux.new && $(TMUX_PREFIX)/bin/tmux -V' \
	    || echo "FAILED"; \
	done

# Build here, then push to every compute node.
tmux-all: tmux tmux-push

# ---------------------------------------------------------------------------
# nodes — install sinteractive onto every compute node: the same set of files
# as install-system, the Claude Code assets included.
#
# /usr/local is node-local, so all of it must be installed on each node
# individually. The assets matter here and not only on the head node, because
# --install-claude resolves them relative to the running script: someone who
# runs it from inside a session is running the node's /usr/local/bin copy, and
# without <prefix>/share/sinteractive beside it that call fails.
#
# If this checkout lives on a cluster-wide mount, with pdsh each node can
# `install` straight from the shared path — note pdsh runs commands, it does
# NOT copy files (that's pdcp). Without pdsh, falls back to piping a tar to
# each node in turn. Run as root (sudo make nodes); this covers the compute
# nodes, the head node gets make install-system.
#
# The script is renamed into place rather than written over, for the reason
# tmux-push does it: a copy of it may be executing on the node right now (an
# --attach SSHes in and runs it there), and truncating a running script in
# place is how you get a half-read one.
# ---------------------------------------------------------------------------
NODELIST = $(shell echo $(NODES) | tr ' ' ',')

# pdsh's built-in default rcmd module is rsh, and nothing listens on 514 here,
# so an unqualified pdsh answers with "connect: Connection refused" for every
# node. A PDSH_RCMD_TYPE=ssh in the admin's own environment does not save this
# target either: sudo resets the environment, so the value never arrives. Ask
# for the module by name instead. Override if a cluster wants another one
# (`make nodes PDSH_RCMD=exec`); `pdsh -V` lists what is compiled in.
PDSH_RCMD ?= ssh

nodes: require-root
	@if command -v pdsh >/dev/null 2>&1; then \
	  pdsh -R $(PDSH_RCMD) -w $(NODELIST) \
	    'install -m 0755 $(CURDIR)/sinteractive /usr/local/bin/sinteractive.new \
	     && mv /usr/local/bin/sinteractive.new /usr/local/bin/sinteractive \
	     && install -D -m 0644 $(CURDIR)/man/sinteractive.1 /usr/local/share/man/man1/sinteractive.1 \
	     && install -D -m 0644 $(CURDIR)/completions/sinteractive.bash /usr/local/share/bash-completion/completions/sinteractive \
	     && install -d -m 0755 /usr/local/share/sinteractive/claude/hooks \
	     && install -m 0755 $(CURDIR)/claude/hooks/*.sh /usr/local/share/sinteractive/claude/hooks/ \
	     && install -m 0644 $(CURDIR)/claude/settings-snippet.json /usr/local/share/sinteractive/claude/ \
	     && for s in $(SKILLS); do install -D -m 0644 $(CURDIR)/$$s /usr/local/share/sinteractive/$$s || exit 1; done \
	     && echo ok'; \
	else \
	  for n in $(NODES); do \
	    printf '==> %s: ' "$$n"; \
	    tar cf - sinteractive man/sinteractive.1 completions/sinteractive.bash \
	          claude/hooks claude/settings-snippet.json $(SKILLS) \
	      | ssh $(SSH_USER)@$$n \
	        'set -e; d=$$(mktemp -d); trap "rm -rf $$d" EXIT; tar xf - -C "$$d"; \
	         install -m 0755 "$$d/sinteractive" /usr/local/bin/sinteractive.new; \
	         mv /usr/local/bin/sinteractive.new /usr/local/bin/sinteractive; \
	         install -D -m 0644 "$$d/man/sinteractive.1" /usr/local/share/man/man1/sinteractive.1; \
	         install -D -m 0644 "$$d/completions/sinteractive.bash" /usr/local/share/bash-completion/completions/sinteractive; \
	         install -d -m 0755 /usr/local/share/sinteractive/claude/hooks; \
	         install -m 0755 "$$d"/claude/hooks/*.sh /usr/local/share/sinteractive/claude/hooks/; \
	         install -m 0644 "$$d/claude/settings-snippet.json" /usr/local/share/sinteractive/claude/; \
	         cd "$$d" && for s in skills/*/SKILL.md; do install -D -m 0644 "$$s" "/usr/local/share/sinteractive/$$s"; done; \
	         echo ok' \
	      || echo "FAILED"; \
	  done; \
	fi

# ---------------------------------------------------------------------------
# nodes-check — report what each node actually has.
#
# Read-only and unprivileged, so it can be run by anyone at any time. A
# fan-out that failed halfway, or a cluster that has not been updated since
# some earlier release, is otherwise invisible: sessions keep working from the
# submitted copy of the script (sbatch spools it), so a stale node only shows
# up in --attach and in --install-claude.
# ---------------------------------------------------------------------------
nodes-check:
	@for n in $(NODES); do \
	  printf '%-12s ' "$$n"; \
	  ssh -o BatchMode=yes -o ConnectTimeout=5 $(SSH_USER)@$$n ' \
	    v=$$(sed -n "s/^VERSION=.\(.*\)./\1/p" /usr/local/bin/sinteractive 2>/dev/null | head -1); \
	    [ -e /usr/local/bin/sinteractive ] || v=missing; \
	    [ -n "$$v" ] || v=unknown; \
	    a=no; [ -d /usr/local/share/sinteractive/skills ] && a=yes; \
	    t=$$(/usr/local/bin/tmux -V 2>/dev/null) || t="tmux missing"; \
	    echo "sinteractive=$$v  assets=$$a  $$t"' 2>/dev/null \
	    || echo "unreachable"; \
	done
