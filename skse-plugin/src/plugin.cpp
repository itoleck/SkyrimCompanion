#include "events.h"
#include "server.h"

namespace
{
    void InitLogging()
    {
        auto dir = logger::log_directory();
        if (!dir) {
            return;
        }
        auto path = *dir / "SkyrimCompanionExporter.log";
        auto sink = std::make_shared<spdlog::sinks::basic_file_sink_mt>(path.string(), true);
        auto log = std::make_shared<spdlog::logger>("global", std::move(sink));
        log->set_level(spdlog::level::info);
        log->flush_on(spdlog::level::info);
        spdlog::set_default_logger(std::move(log));
    }
}

SKSEPluginLoad(const SKSE::LoadInterface* skse)
{
    InitLogging();
    SKSE::Init(skse);
    logger::info("Skyrim Companion exporter {} loading, game {}", COMPANION_VERSION, REL::Module::get().version().string());

    SKSE::GetMessagingInterface()->RegisterListener([](SKSE::MessagingInterface::Message* msg) {
        switch (msg->type) {
        case SKSE::MessagingInterface::kDataLoaded:
            companion::events::Install();
            companion::StartServer();
            break;
        case SKSE::MessagingInterface::kPostLoadGame:
        case SKSE::MessagingInterface::kNewGame:
            companion::events::OnGameLoaded();
            break;
        default:
            break;
        }
    });
    return true;
}
