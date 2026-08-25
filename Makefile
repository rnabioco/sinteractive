PREFIX ?= ~/.local/bin
# man finds ~/.local/share/man automatically when ~/.local/bin is on PATH.
MANDIR ?= ~/.local/share/man/man1
# bash-completion's dynamic loader searches $XDG_DATA_HOME (user) and
# $XDG_DATA_DIRS (which includes /usr/local/share) for
# bash-completion/completions/<command>.
COMPDIR ?= ~/.local/share/bash-completion/completions
# Claude Code assets (skill + hooks) ship beside the script so that
# `sinteractive --install-claude` works from an installed copy and not only
# from a git checkout. Mirrors the repo layout, so the lookup in the script
# has one code path for both.
SHAREDIR ?= ~/.local/share/sinteractive

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
	mkdir -p $(SHAREDIR)/claude/hooks $(SHAREDIR)/skills/bodhi-compute
	install -m 0755 claude/hooks/*.sh $(SHAREDIR)/claude/hooks/
	install -m 0644 claude/settings-snippet.json $(SHAREDIR)/claude/
	install -m 0644 skills/bodhi-compute/SKILL.md $(SHAREDIR)/skills/bodhi-compute/

install-system:
	install -m 0755 sinteractive /usr/local/bin/sinteractive
	install -D -m 0644 man/sinteractive.1 /usr/local/share/man/man1/sinteractive.1
	install -D -m 0644 completions/sinteractive.bash /usr/local/share/bash-completion/completions/sinteractive
	install -d -m 0755 /usr/local/share/sinteractive/claude/hooks /usr/local/share/sinteractive/skills/bodhi-compute
	install -m 0755 claude/hooks/*.sh /usr/local/share/sinteractive/claude/hooks/
	install -m 0644 claude/settings-snippet.json /usr/local/share/sinteractive/claude/
	install -m 0644 skills/bodhi-compute/SKILL.md /usr/local/share/sinteractive/skills/bodhi-compute/

# ---------------------------------------------------------------------------
# Claude Code integration. Two parts:
#
#   - the bodhi-compute skill, which teaches an agent cluster etiquette: the
#     login node and an sinteractive session are both orchestration shells,
#     and real work goes into its own allocation;
#   - two hooks for an agent running INSIDE a session, which tell it at
#     startup where it is and how big the allocation is, and warn it when the
#     session is running out of walltime.
#
# All per-user, so this installs into ~/.claude regardless of UID.
#
# settings.json is printed, not merged: it is the user's file and usually
# already carries hooks of their own, so clobbering it is not this Makefile's
# call to make.
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

.PHONY: tmux-deps tmux tmux-push tmux-all nodes require-root

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
# nodes — install sinteractive and its man page onto every compute node.
#
# /usr/local is node-local, so the script and man page must be installed on
# each node individually. If this checkout lives on a cluster-wide mount, with
# pdsh each node can `install` straight from the shared path — note pdsh runs
# commands, it does NOT copy files (that's pdcp). Without pdsh, falls back to
# the same scp/ssh loop as tmux-push. Run as root (sudo make nodes).
# ---------------------------------------------------------------------------
NODELIST = $(shell echo $(NODES) | tr ' ' ',')

nodes: require-root
	@if command -v pdsh >/dev/null 2>&1; then \
	  pdsh -w $(NODELIST) \
	    'install -m 0755 $(CURDIR)/sinteractive /usr/local/bin/sinteractive \
	     && install -D -m 0644 $(CURDIR)/man/sinteractive.1 /usr/local/share/man/man1/sinteractive.1 \
	     && install -D -m 0644 $(CURDIR)/completions/sinteractive.bash /usr/local/share/bash-completion/completions/sinteractive \
	     && echo ok'; \
	else \
	  for n in $(NODES); do \
	    printf '==> %s: ' "$$n"; \
	    scp -q sinteractive man/sinteractive.1 completions/sinteractive.bash $(SSH_USER)@$$n:/tmp/ \
	      && ssh $(SSH_USER)@$$n \
	        'install -m 0755 /tmp/sinteractive /usr/local/bin/sinteractive \
	         && install -D -m 0644 /tmp/sinteractive.1 /usr/local/share/man/man1/sinteractive.1 \
	         && install -D -m 0644 /tmp/sinteractive.bash /usr/local/share/bash-completion/completions/sinteractive \
	         && rm -f /tmp/sinteractive /tmp/sinteractive.1 /tmp/sinteractive.bash && echo ok' \
	      || echo "FAILED"; \
	  done; \
	fi
