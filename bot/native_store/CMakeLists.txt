cmake_minimum_required(VERSION 3.20)
project(signum_store LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
find_package(CURL REQUIRED)
find_package(OpenSSL REQUIRED)
find_package(nlohmann_json REQUIRED)
find_package(fmt REQUIRED)
find_library(UNICORN_LIBRARY unicorn REQUIRED)
find_library(MINIZIP_LIBRARY minizip REQUIRED)
add_executable(signum-store main.cpp protect.cpp hwid.cpp device_mac.cpp appstore.cpp
    http_client.cpp plist.cpp sha2.cpp aes.cpp MachImage.cpp SapMachine.cpp SapSigner.cpp StoreAgentMachine.cpp)
target_include_directories(signum-store PRIVATE ${UNICORN_INCLUDE_DIR})
target_compile_definitions(signum-store PRIVATE HAVE_MINIZIP FMT_HEADER_ONLY SAP_ASSETS_EMBEDDED)
target_link_libraries(signum-store PRIVATE CURL::libcurl OpenSSL::SSL OpenSSL::Crypto
    nlohmann_json::nlohmann_json fmt::fmt-header-only ${UNICORN_LIBRARY} ${MINIZIP_LIBRARY})
set_target_properties(signum-store PROPERTIES INSTALL_RPATH "$ORIGIN")
install(TARGETS signum-store RUNTIME DESTINATION bin)
foreach(asset CoreFP CommerceCore CommerceKit CoreFP.icxs storeagent)
    string(REPLACE "." "_" symbol "${asset}")
    set(object "${CMAKE_BINARY_DIR}/${symbol}.o")
    add_custom_command(OUTPUT ${object} COMMAND objcopy --input-target binary
        --output-target elf64-x86-64 --binary-architecture i386:x86-64 ${asset} ${object}
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/sap_assets
        DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/sap_assets/${asset})
    target_sources(signum-store PRIVATE ${object})
endforeach()
