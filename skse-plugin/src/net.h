#pragma once

// Plain WinSock code, kept apart from CommonLib headers (their Windows
// declarations clash with WinSock2.h). Do not include RE/ or SKSE/ here.

#include <chrono>
#include <cstdint>
#include <functional>
#include <string>

namespace companion::net
{
    struct Callbacks
    {
        // Called once per interval while at least one client is connected.
        std::function<void()> onTick;
        // Returns the newest newline-terminated JSON line, or empty if none yet.
        std::function<std::string()> latest;
        std::function<void(const std::string&)> info;
        std::function<void(const std::string&)> error;
    };

    // Blocks forever serving 127.0.0.1:port.
    void Serve(std::uint16_t port, std::chrono::milliseconds interval, const Callbacks& cb);
}
