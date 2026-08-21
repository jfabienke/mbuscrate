# Distributed under the OSI-approved BSD 3-Clause License.  See accompanying
# file LICENSE.rst or https://cmake.org/licensing for details.

cmake_minimum_required(VERSION ${CMAKE_VERSION}) # this file comes with cmake

# If CMAKE_DISABLE_SOURCE_CHANGES is set to true and the source directory is an
# existing directory in our source tree, calling file(MAKE_DIRECTORY) on it
# would cause a fatal error, even though it would be a no-op.
if(NOT EXISTS "/private/tmp/claude-501/-Users-jvindahl-Development-mbuscrate/f26d9b84-c014-458a-8936-77908afa4d57/scratchpad/pico2/pico-sdk/tools/pioasm")
  file(MAKE_DIRECTORY "/private/tmp/claude-501/-Users-jvindahl-Development-mbuscrate/f26d9b84-c014-458a-8936-77908afa4d57/scratchpad/pico2/pico-sdk/tools/pioasm")
endif()
file(MAKE_DIRECTORY
  "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pioasm"
  "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pioasm-install"
  "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pico-sdk/src/rp2_common/pico_status_led/pioasm/tmp"
  "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pico-sdk/src/rp2_common/pico_status_led/pioasm/src/pioasmBuild-stamp"
  "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pico-sdk/src/rp2_common/pico_status_led/pioasm/src"
  "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pico-sdk/src/rp2_common/pico_status_led/pioasm/src/pioasmBuild-stamp"
)

set(configSubDirs )
foreach(subDir IN LISTS configSubDirs)
    file(MAKE_DIRECTORY "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pico-sdk/src/rp2_common/pico_status_led/pioasm/src/pioasmBuild-stamp/${subDir}")
endforeach()
if(cfgdir)
  file(MAKE_DIRECTORY "/Users/jvindahl/Development/mbuscrate/tools/pico-lora-beacon/build2/pico-sdk/src/rp2_common/pico_status_led/pioasm/src/pioasmBuild-stamp${cfgdir}") # cfgdir has leading slash
endif()
