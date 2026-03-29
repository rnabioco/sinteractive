PREFIX ?= ~/.local/bin

.PHONY: install

install:
	mkdir -p $(PREFIX)
	cp scripts/sinteractive $(PREFIX)/sinteractive
	chmod +x $(PREFIX)/sinteractive
	cp scripts/bodhi-splash $(PREFIX)/bodhi-splash
	chmod +x $(PREFIX)/bodhi-splash
