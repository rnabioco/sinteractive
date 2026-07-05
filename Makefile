PREFIX ?= ~/.local/bin

# When run as root, install system-wide: sinteractive to /usr/local/bin and
# the splash to /etc/profile.d so it runs for every interactive login.
UID := $(shell id -u)

.PHONY: install install-user install-system skill-install

ifeq ($(UID),0)
install: install-system
else
install: install-user
endif

install-user:
	mkdir -p $(PREFIX)
	cp scripts/sinteractive $(PREFIX)/sinteractive
	chmod +x $(PREFIX)/sinteractive
	cp scripts/bodhi-splash $(PREFIX)/bodhi-splash
	chmod +x $(PREFIX)/bodhi-splash

install-system:
	install -m 0755 scripts/sinteractive /usr/local/bin/sinteractive
	install -m 0644 scripts/bodhi-splash /etc/profile.d/bodhi-splash.sh

# Claude Code skill: teaches agents to run heavy work in a Slurm allocation
# (sinteractive --detach/--status, srun --overlap, time budgets). Skills are
# per-user, so this installs into ~/.claude/skills regardless of UID.
skill-install:
	mkdir -p $(HOME)/.claude/skills/bodhi-compute
	cp skills/bodhi-compute/SKILL.md $(HOME)/.claude/skills/bodhi-compute/SKILL.md

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
TMUX_VERSION     ?= 3.7b
TMUX_PREFIX      ?= /usr/local
TMUX_URL          = https://github.com/tmux/tmux/releases/download/$(TMUX_VERSION)/tmux-$(TMUX_VERSION).tar.gz
TMUX_BUILD_DIR   ?= /tmp/tmux-build-$(TMUX_VERSION)
CONFIGURE_FLAGS  ?=

# Compute nodes to push the built binary to (this head/login node builds it).
# Defaults to every node Slurm knows about; override with `make tmux-push NODES="compute00 compute01"`.
NODES            ?= $(shell sinfo -hN -o '%N' 2>/dev/null | sort -u)
SSH_USER         ?= root

.PHONY: tmux-deps tmux tmux-push tmux-all require-root

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
