# Pinned TIFF codecs, built with CMake only. The outer SDK build uses two jobs;
# each ExternalProject has one build job so concurrent dependencies stay bounded.
include(ExternalProject)
set(tiff_dep_root "${CMAKE_BINARY_DIR}/tiff-deps")
set(tiff_common -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=${CMAKE_INSTALL_PREFIX}
  -DCMAKE_INSTALL_LIBDIR=lib -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTING=OFF
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON -DCMAKE_C_COMPILER=${CMAKE_C_COMPILER}
  -DCMAKE_CXX_COMPILER=${CMAKE_CXX_COMPILER})
ExternalProject_Add(tiff_zlib SOURCE_DIR "${ZLIB_SOURCE}" BINARY_DIR "${tiff_dep_root}/zlib"
  CMAKE_ARGS ${tiff_common}
  BUILD_COMMAND ${CMAKE_COMMAND} --build <BINARY_DIR> --target zlibstatic --parallel 1
  INSTALL_COMMAND ${CMAKE_COMMAND} -E make_directory "${CMAKE_INSTALL_PREFIX}/lib" "${CMAKE_INSTALL_PREFIX}/include"
  COMMAND ${CMAKE_COMMAND} -E copy <BINARY_DIR>/libz.a "${CMAKE_INSTALL_PREFIX}/lib/libz.a"
  COMMAND ${CMAKE_COMMAND} -E copy <SOURCE_DIR>/zlib.h <BINARY_DIR>/zconf.h "${CMAKE_INSTALL_PREFIX}/include")
ExternalProject_Add(tiff_lzma SOURCE_DIR "${XZ_SOURCE}" BINARY_DIR "${tiff_dep_root}/lzma"
  CMAKE_ARGS ${tiff_common} -DXZ_NLS=OFF -DXZ_DOC=OFF -DXZ_TOOL_XZ=OFF
    -DXZ_TOOL_XZDEC=OFF -DXZ_TOOL_LZMADEC=OFF -DXZ_TOOL_LZMAINFO=OFF
    -DXZ_TOOL_SCRIPTS=OFF -DXZ_DOXYGEN=OFF
  BUILD_COMMAND ${CMAKE_COMMAND} --build <BINARY_DIR> --target liblzma --parallel 1)
ExternalProject_Add(tiff_zstd SOURCE_DIR "${ZSTD_SOURCE}/build/cmake" BINARY_DIR "${tiff_dep_root}/zstd"
  CMAKE_ARGS ${tiff_common} -DZSTD_BUILD_PROGRAMS=OFF -DZSTD_BUILD_TESTS=OFF
    -DZSTD_BUILD_CONTRIB=OFF -DZSTD_BUILD_SHARED=OFF -DZSTD_BUILD_STATIC=ON
  BUILD_COMMAND ${CMAKE_COMMAND} --build <BINARY_DIR> --parallel 1)
ExternalProject_Add(tiff_jpeg SOURCE_DIR "${JPEG_SOURCE}" BINARY_DIR "${tiff_dep_root}/jpeg"
  CMAKE_ARGS ${tiff_common} -DENABLE_SHARED=OFF -DENABLE_STATIC=ON -DWITH_TURBOJPEG=OFF
    -DWITH_TOOLS=OFF -DWITH_TESTS=OFF -DWITH_SIMD=OFF
  BUILD_COMMAND ${CMAKE_COMMAND} --build <BINARY_DIR> --target jpeg-static --parallel 1)
ExternalProject_Add(tiff_codec SOURCE_DIR "${TIFF_SOURCE}" BINARY_DIR "${tiff_dep_root}/tiff"
  DEPENDS tiff_zlib tiff_lzma tiff_zstd tiff_jpeg
  CMAKE_ARGS ${tiff_common} -Dtiff-static=ON -Dtiff-tools=OFF -Dtiff-tests=OFF
    -Dtiff-contrib=OFF -Dtiff-docs=OFF -Dtiff-deprecated=OFF -Dtiff-install=ON
    -Dzlib=ON -Djpeg=ON -Dold-jpeg=ON -Dlzma=ON -Dzstd=ON -Dlibdeflate=OFF
    -Dwebp=OFF -Djbig=OFF -Dlerc=OFF -Dcxx=OFF -Dopengl=OFF
    -DCMAKE_DISABLE_FIND_PACKAGE_Deflate=TRUE -DCMAKE_DISABLE_FIND_PACKAGE_WebP=TRUE
    -DCMAKE_DISABLE_FIND_PACKAGE_JBIG=TRUE -DCMAKE_DISABLE_FIND_PACKAGE_LERC=TRUE
    -DZLIB_LIBRARY=${CMAKE_INSTALL_PREFIX}/lib/libz.a -DZLIB_INCLUDE_DIR=${CMAKE_INSTALL_PREFIX}/include
    -DJPEG_LIBRARY=${CMAKE_INSTALL_PREFIX}/lib/libjpeg.a -DJPEG_INCLUDE_DIR=${CMAKE_INSTALL_PREFIX}/include
    -DLIBLZMA_LIBRARY=${CMAKE_INSTALL_PREFIX}/lib/liblzma.a -DLIBLZMA_INCLUDE_DIR=${CMAKE_INSTALL_PREFIX}/include
    -DZSTD_LIBRARY=${CMAKE_INSTALL_PREFIX}/lib/libzstd.a -DZSTD_INCLUDE_DIR=${CMAKE_INSTALL_PREFIX}/include
  BUILD_COMMAND ${CMAKE_COMMAND} --build <BINARY_DIR> --target tiff --parallel 1)
