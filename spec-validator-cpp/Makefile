# Build the standalone .upack validator.
#
#   make                    — build the validator
#   make clean              — remove build outputs
#   make test               — build, run slippypack against synthetic,
#                             validate the produced pack

CXX      ?= c++
CXXFLAGS ?= -std=c++17 -O2 -Wall -Wextra -Wpedantic -Wshadow \
            -Wconversion -Wsign-conversion

BIN_DIR  := build
BIN      := $(BIN_DIR)/upack_validate
SRC      := src/validator.cpp

.PHONY: all clean test

all: $(BIN)

$(BIN): $(SRC) | $(BIN_DIR)
	$(CXX) $(CXXFLAGS) -o $@ $<

$(BIN_DIR):
	mkdir -p $@

clean:
	rm -rf $(BIN_DIR)

test: $(BIN)
	@bash tests/run.sh
