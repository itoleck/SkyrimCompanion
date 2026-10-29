#pragma once

#include <cstdint>

namespace companion
{
    inline constexpr std::uint16_t kDefaultPort = 47311;

    // Starts a background thread that serves snapshots on 127.0.0.1:port.
    void StartServer(std::uint16_t port = kDefaultPort);
}
