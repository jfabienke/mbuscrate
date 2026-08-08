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
