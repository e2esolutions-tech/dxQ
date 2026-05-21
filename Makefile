# Sezar — top-level convenience Makefile.
#
# Useful targets:
#   make test                — cargo test --workspace
#   make release             — cargo build --release on every binary
#   make systemd-install     — install release binaries + unit files
#                              (requires sudo; idempotent)
#   make systemd-uninstall   — stop, disable, remove all units + binaries
#   make acceptance          — scripts/acceptance.sh (V1 gate)
#   make loadtest            — scripts/loadtest.py against localhost
#
# All targets are documentation-friendly: `make <target>` echoes
# what it's about to do before running it.

PREFIX     ?= /usr/local
BINDIR     ?= $(PREFIX)/bin
SYSTEMDDIR ?= /etc/systemd/system
SUDO       ?= sudo

BINARIES   := sezar-server sezar-net sezar-cert sezar-chain sezar-id sezar-agility
UNITS_SVC  := sezar-server.service sezar-net-live.service \
              sezar-cert-host-scan.service sezar-id-inventory.service
UNITS_TMR  := sezar-cert-host-scan.timer sezar-id-inventory.timer

.PHONY: help test release acceptance loadtest \
        paper paper-submission paper-submission-extended paper-submission-both \
        systemd-install systemd-uninstall \
        check-systemd verify-units

help:
	@echo "Sezar Makefile — common targets:"
	@grep -E '^[a-zA-Z][a-zA-Z0-9_-]*:.*##' Makefile | sort | \
	    awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-22s\033[0m %s\n",$$1,$$2}'

test: ## cargo test --workspace
	cargo test --workspace

release: ## cargo build --release on every binary
	cargo build --release --workspace

acceptance: release ## V1 acceptance smoke against release binaries
	./scripts/acceptance.sh

loadtest: ## scripts/loadtest.py against http://127.0.0.1:8090/v1/events
	./scripts/loadtest.py

paper: ## rebuild paper PDFs (magazine + extended)
	cd docs/paper && ./build.sh

paper-submission: ## bundle the paper for venue upload (magazine default)
	./scripts/paper-submission-package.sh magazine

paper-submission-extended: ## bundle the extended variant
	./scripts/paper-submission-package.sh extended

paper-submission-both: ## bundle magazine + extended variants
	./scripts/paper-submission-package.sh both

systemd-install: release ## install binaries + unit files (sudo)
	@echo "→ installing binaries to $(BINDIR)"
	@for b in $(BINARIES); do \
	    if [ -f target/release/$$b ]; then \
	        $(SUDO) install -m 0755 target/release/$$b $(BINDIR)/$$b; \
	    fi; \
	done
	@echo "→ installing unit files to $(SYSTEMDDIR)"
	@for u in $(UNITS_SVC) $(UNITS_TMR); do \
	    $(SUDO) install -m 0644 dist/systemd/$$u $(SYSTEMDDIR)/$$u; \
	done
	@echo "→ creating sezar system user + state dirs"
	@id sezar >/dev/null 2>&1 || $(SUDO) useradd -r -s /sbin/nologin sezar
	@$(SUDO) install -d -m 0750 -o sezar -g sezar /var/lib/sezar /var/lib/sezar/ca
	@$(SUDO) install -d -m 0750 -o sezar -g sezar /var/lib/sezar-net /var/lib/sezar-net/spool
	@echo "→ systemctl daemon-reload"
	@$(SUDO) systemctl daemon-reload
	@echo
	@echo "Installed. Operator-side next:"
	@echo "  1. Drop in /etc/systemd/system/sezar-server.service.d/override.conf"
	@echo "     with SEZAR_DATABASE_URL + SEZAR_ADMIN_TOKEN."
	@echo "  2. sudo systemctl enable --now sezar-server"
	@echo "  3. See docs/operator-deploy.md for the agent-side dance."

systemd-uninstall: ## stop + disable + remove every unit and binary
	@echo "→ stopping units"
	@for u in $(UNITS_SVC) $(UNITS_TMR); do \
	    $(SUDO) systemctl disable --now $$u 2>/dev/null || true; \
	done
	@echo "→ removing unit files"
	@for u in $(UNITS_SVC) $(UNITS_TMR); do \
	    $(SUDO) rm -f $(SYSTEMDDIR)/$$u; \
	done
	@echo "→ removing binaries"
	@for b in $(BINARIES); do \
	    $(SUDO) rm -f $(BINDIR)/$$b; \
	done
	@$(SUDO) systemctl daemon-reload
	@echo
	@echo "Removed. State dirs and the 'sezar' user are left alone — clean"
	@echo "them up manually with:"
	@echo "  sudo rm -rf /var/lib/sezar /var/lib/sezar-net"
	@echo "  sudo userdel sezar"

check-systemd: ## systemd-analyze verify every unit
	@for u in $(UNITS_SVC) $(UNITS_TMR); do \
	    echo "→ verifying $$u"; \
	    systemd-analyze verify dist/systemd/$$u; \
	done

verify-units: check-systemd ## alias for check-systemd
