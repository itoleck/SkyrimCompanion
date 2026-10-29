// Snapshots are built on the game thread via the SKSE task queue and handed
// to the network thread through a mutex.

#include "server.h"
#include "net.h"
#include "snapshot.h"

namespace companion
{
    namespace
    {
        constexpr auto kInterval = 250ms;

        std::mutex g_latestLock;
        std::string g_latest;
        std::atomic_bool g_taskPending{ false };

        void RequestSnapshot()
        {
            // Skip if the game thread hasn't run the previous request yet (e.g. during a loading screen).
            if (g_taskPending.exchange(true)) {
                return;
            }
            SKSE::GetTaskInterface()->AddTask([] {
                auto json = BuildSnapshotJson();
                json.push_back('\n');
                {
                    std::scoped_lock lock(g_latestLock);
                    g_latest = std::move(json);
                }
                g_taskPending = false;
            });
        }
    }

    void StartServer(std::uint16_t port)
    {
        std::thread([port] {
            net::Serve(port, kInterval, {
                .onTick = RequestSnapshot,
                .latest = [] {
                    std::scoped_lock lock(g_latestLock);
                    return g_latest;
                },
                .info = [](const std::string& m) { logger::info("{}", m); },
                .error = [](const std::string& m) { logger::error("{}", m); },
            });
        }).detach();
    }
}
