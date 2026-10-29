#pragma once

#include <RE/Skyrim.h>
#include <SKSE/SKSE.h>
#include <spdlog/sinks/basic_file_sink.h>

#include <atomic>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace logger = SKSE::log;
using namespace std::literals;
