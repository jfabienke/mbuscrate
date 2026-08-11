cmake_minimum_required(VERSION 3.18)
include(pico_sdk_import.cmake)

project(pico-lora-beacon C CXX ASM)
set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)
pico_sdk_init()

add_compile_options(-Wall -Wno-format -Wno-unused-function)

# RADIOLIB_DIR is passed on the cmake command line so this tree stays free of a
# vendored copy of the library.
add_subdirectory("${RADIOLIB_DIR}" "${CMAKE_CURRENT_BINARY_DIR}/RadioLib")

add_executable(${PROJECT_NAME}
    main.cpp
    "${RADIOLIB_DIR}/src/hal/RPiPico/PicoHal.cpp"
)
target_compile_definitions(${PROJECT_NAME} PUBLIC RADIOLIB_BUILD_RPI_PICO)
target_link_libraries(${PROJECT_NAME}
    pico_stdlib hardware_spi hardware_gpio hardware_timer pico_multicore hardware_pwm RadioLib)

pico_enable_stdio_usb(${PROJECT_NAME} 1)
pico_enable_stdio_uart(${PROJECT_NAME} 0)
pico_add_extra_outputs(${PROJECT_NAME})

# Receiver variant, for proving the gateway's transmit path. Same source, so the
# two ends cannot drift apart in their air parameters.
add_executable(pico-lora-rx
    main.cpp
    "${RADIOLIB_DIR}/src/hal/RPiPico/PicoHal.cpp"
)
target_compile_definitions(pico-lora-rx PUBLIC RADIOLIB_BUILD_RPI_PICO PICO_LORA_ROLE_RX)
target_link_libraries(pico-lora-rx
    pico_stdlib hardware_spi hardware_gpio hardware_timer pico_multicore hardware_pwm RadioLib)
pico_enable_stdio_usb(pico-lora-rx 1)
pico_enable_stdio_uart(pico-lora-rx 0)
pico_add_extra_outputs(pico-lora-rx)

# LoRaWAN meter emulator. Credentials come from the cmake command line so no key
# is ever committed:
#   cmake -DMETER_DEV_EUI=0x... -DMETER_APP_KEY="0x2B,0x7E,..." ..
add_executable(pico-lora-meter
    meter.cpp
    "${RADIOLIB_DIR}/src/hal/RPiPico/PicoHal.cpp"
)
target_compile_definitions(pico-lora-meter PUBLIC RADIOLIB_BUILD_RPI_PICO)
if(METER_DEV_EUI)
    target_compile_definitions(pico-lora-meter PUBLIC METER_DEV_EUI=${METER_DEV_EUI})
endif()
if(METER_JOIN_EUI)
    target_compile_definitions(pico-lora-meter PUBLIC METER_JOIN_EUI=${METER_JOIN_EUI})
endif()
if(METER_APP_KEY)
    target_compile_definitions(pico-lora-meter PUBLIC METER_APP_KEY=${METER_APP_KEY})
endif()
target_link_libraries(pico-lora-meter
    pico_stdlib hardware_spi hardware_gpio hardware_timer hardware_adc pico_multicore hardware_pwm RadioLib)
pico_enable_stdio_usb(pico-lora-meter 1)
pico_enable_stdio_uart(pico-lora-meter 0)
pico_add_extra_outputs(pico-lora-meter)

# LoRaWAN join tester — an OTAA end-device that reports DevNonce and JoinNonce
# per join. 'j' uses RadioLib's monotonic counter (the 1.0.4 model); 'r' injects a
# random DevNonce (the 1.0.2 model the real fleet uses — RadioLib cannot emit
# these natively); 'p' replays the last random nonce (must be rejected).
# Credentials come from the cmake command line (-DJT_DEV_EUI=... etc.); the
# defaults match meter.cpp and the gateway's join-creds.json.
add_executable(pico-lora-jointest
    jointest.cpp
    "${RADIOLIB_DIR}/src/hal/RPiPico/PicoHal.cpp"
)
target_compile_definitions(pico-lora-jointest PUBLIC RADIOLIB_BUILD_RPI_PICO)
if(JT_DEV_EUI)
    target_compile_definitions(pico-lora-jointest PUBLIC JT_DEV_EUI=${JT_DEV_EUI})
endif()
if(JT_JOIN_EUI)
    target_compile_definitions(pico-lora-jointest PUBLIC JT_JOIN_EUI=${JT_JOIN_EUI})
endif()
if(JT_APP_KEY)
    target_compile_definitions(pico-lora-jointest PUBLIC JT_APP_KEY=${JT_APP_KEY})
endif()
# pico_rand: 'r' draws its random DevNonce from get_rand_32().
target_link_libraries(pico-lora-jointest
    pico_stdlib pico_rand hardware_spi hardware_gpio hardware_timer pico_multicore hardware_pwm RadioLib)
pico_enable_stdio_usb(pico-lora-jointest 1)
pico_enable_stdio_uart(pico-lora-jointest 0)
pico_add_extra_outputs(pico-lora-jointest)
