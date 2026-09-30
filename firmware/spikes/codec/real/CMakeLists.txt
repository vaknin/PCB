# Phase D.5: the codec spike on a real ESP32-S3 (N16R8) dev board. A separate project, so the
# QEMU spike next to it (../main, ../run.sh) and its build stay as they are. See ../run-real.sh.
cmake_minimum_required(VERSION 3.22)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(codec_spike_real)
