PREFIX ?= ~/.local/bin

# When run as root, install system-wide: sinteractive to /usr/local/bin and
# the splash to /etc/profile.d so it runs for every interactive login.
UID := $(shell id -u)

.PHONY: install install-user install-system

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
