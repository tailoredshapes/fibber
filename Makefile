# fibber: GNU Make 4 is the build coordinator (docs/design/build.md). Targets are files; `make help` lists the public ones.
#   make -j8 gate        the full gate, incremental (the stamps under build/ say what passed on which inputs)
#   make -j8 quick       the quick gate
#   make release         dist/fibc-VERSION-PLATFORM.tar.gz and SHA256SUMS
# macOS: /usr/bin/make is GNU Make 3.81 and stops at once with a message; `brew install make` and run `gmake` (`gmake mac-check`: mk/mac.mk).
# MK is the directory of this Makefile's sections, so that a copy of the Makefile and mk/ elsewhere (compiler/tests/make/graph.sh plants
# faults in one) runs against this tree with `make -f COPY/Makefile`.
MK := $(dir $(lastword $(MAKEFILE_LIST)))mk
include $(MK)/config.mk
include $(MK)/seed.mk
include $(MK)/stage2.mk
include $(MK)/golden.mk
include $(MK)/cases.mk
include $(MK)/tools.mk
include $(MK)/checks.mk
include $(MK)/gate.mk
include $(MK)/release.mk
include $(MK)/mac.mk
include $(MK)/mutants.mk
include $(MK)/bench.mk
include $(MK)/k8s.mk
include $(MK)/fetch.mk
include $(MK)/help.mk
