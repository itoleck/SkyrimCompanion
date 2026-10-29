// Notable things the player does (kills, theft, standing stones, ...), kept in a small ring
// buffer that every snapshot repeats. Each event has a sequence number so the app can skip
// the ones it has already saved. Field names must match GameEvent in crates/protocol.

#include "events.h"

#include <chrono>
#include <deque>
#include <format>
#include <map>
#include <mutex>

using json = nlohmann::json;

namespace companion::events
{
    namespace
    {
        constexpr std::size_t kKeep = 100;
        constexpr RE::FormID kPlayerId = 0x14;
        constexpr RE::FormID kTamrielWorldspace = 0x0000003C;

        std::mutex g_lock;
        std::deque<json> g_events;
        std::uint64_t g_seq = 0;
        const std::uint64_t g_session = static_cast<std::uint64_t>(
            std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::system_clock::now().time_since_epoch()).count());

        std::map<RE::FormID, std::int32_t> g_bounties;
        bool g_bountiesKnown = false;

        std::string SafeStr(const char* s) { return s ? s : ""; }

        // Records an event at `where` (the player's position when null), in the player's current space.
        void Add(const char* kind, std::string text, const RE::NiPoint3* where = nullptr)
        {
            auto* player = RE::PlayerCharacter::GetSingleton();
            if (!player) {
                return;
            }
            auto* cell = player->GetParentCell();
            const bool interior = cell && cell->IsInteriorCell();
            std::string worldspace;
            if (auto* ws = player->GetWorldspace(); ws && !interior) {
                worldspace = ws->GetFormID() == kTamrielWorldspace ? "Tamriel" : SafeStr(ws->GetFormEditorID());
            }
            const auto pos = where ? *where : player->GetPosition();
            auto* cal = RE::Calendar::GetSingleton();

            std::scoped_lock lock(g_lock);
            g_events.push_back({
                { "seq", ++g_seq },
                { "kind", kind },
                { "text", std::move(text) },
                { "player", SafeStr(player->GetName()) },
                { "position", { { "x", pos.x }, { "y", pos.y }, { "z", pos.z } } },
                { "worldspace", worldspace },
                { "is_interior", interior },
                { "days_passed", cal ? cal->GetDaysPassed() : 0.0f },
            });
            if (g_events.size() > kKeep) {
                g_events.pop_front();
            }
            logger::info("event {}: {}", kind, g_events.back()["text"].get<std::string>());
        }

        bool MenuOpen(std::string_view name)
        {
            auto* ui = RE::UI::GetSingleton();
            return ui && ui->IsMenuOpen(name);
        }

        class Sinks final :
            public RE::BSTEventSink<RE::ActorKill::Event>,
            public RE::BSTEventSink<RE::TESActivateEvent>,
            public RE::BSTEventSink<RE::TESContainerChangedEvent>,
            public RE::BSTEventSink<RE::ItemsPickpocketed::Event>,
            public RE::BSTEventSink<RE::LocationDiscovery::Event>,
            public RE::BSTEventSink<RE::LocationCleared::Event>,
            public RE::BSTEventSink<RE::DragonSoulsGained::Event>,
            public RE::BSTEventSink<RE::LevelIncrease::Event>
        {
            using Result = RE::BSEventNotifyControl;

        public:
            static Sinks* Get()
            {
                static Sinks sinks;
                return &sinks;
            }

            Result ProcessEvent(const RE::ActorKill::Event* e, RE::BSTEventSource<RE::ActorKill::Event>*) override
            {
                if (e && e->victim && e->killer && e->killer->IsPlayerRef()) {
                    const auto pos = e->victim->GetPosition();
                    Add("kill", std::format("Killed {}", SafeStr(e->victim->GetDisplayFullName())), &pos);
                }
                return Result::kContinue;
            }

            Result ProcessEvent(const RE::TESActivateEvent* e, RE::BSTEventSource<RE::TESActivateEvent>*) override
            {
                if (!e || !e->actionRef || !e->actionRef->IsPlayerRef() || !e->objectActivated) {
                    return Result::kContinue;
                }
                auto* base = e->objectActivated->GetBaseObject();
                if (!base || !base->Is(RE::FormType::Activator)) {
                    return Result::kContinue;
                }
                const std::string name = SafeStr(e->objectActivated->GetDisplayFullName());
                if (name.ends_with(" Stone") || name.ends_with("stone")) {
                    Add("standing_stone", std::format("Activated {}", name));
                } else if (name.starts_with("Shrine")) {
                    Add("shrine", std::format("Prayed at {}", name));
                }
                return Result::kContinue;
            }

            // Theft: items moving to the player from something the player doesn't own.
            Result ProcessEvent(const RE::TESContainerChangedEvent* e, RE::BSTEventSource<RE::TESContainerChangedEvent>*) override
            {
                if (!e || e->newContainer != kPlayerId || e->itemCount <= 0 ||
                    MenuOpen(RE::BarterMenu::MENU_NAME) || MenuOpen(RE::GiftMenu::MENU_NAME)) {
                    return Result::kContinue;
                }
                auto* player = RE::PlayerCharacter::GetSingleton();
                RE::TESObjectREFR* from = nullptr;
                if (e->oldContainer != 0) {
                    from = RE::TESForm::LookupByID<RE::TESObjectREFR>(e->oldContainer);
                    // Taking from people is pickpocketing (its own event) or looting; neither is theft here.
                    if (from && from->Is(RE::FormType::ActorCharacter)) {
                        return Result::kContinue;
                    }
                } else {
                    from = e->reference.get().get();
                }
                if (!player || !from || !player->WouldBeStealing(from)) {
                    return Result::kContinue;
                }
                auto* item = RE::TESForm::LookupByID(e->baseObj);
                const std::string name = item ? SafeStr(item->GetName()) : "something";
                Add("steal", e->itemCount > 1 ? std::format("Stole {} x{}", name, e->itemCount) : std::format("Stole {}", name));
                return Result::kContinue;
            }

            Result ProcessEvent(const RE::ItemsPickpocketed::Event* e, RE::BSTEventSource<RE::ItemsPickpocketed::Event>*) override
            {
                if (e && e->numItems > 0) {
                    Add("pickpocket", e->numItems > 1 ? std::format("Pickpocketed {} items", e->numItems) : "Pickpocketed an item");
                }
                return Result::kContinue;
            }

            Result ProcessEvent(const RE::LocationDiscovery::Event* e, RE::BSTEventSource<RE::LocationDiscovery::Event>*) override
            {
                if (e && e->mapMarkerData) {
                    Add("discovered", std::format("Discovered {}", SafeStr(e->mapMarkerData->locationName.GetFullName())));
                }
                return Result::kContinue;
            }

            Result ProcessEvent(const RE::LocationCleared::Event*, RE::BSTEventSource<RE::LocationCleared::Event>*) override
            {
                auto* player = RE::PlayerCharacter::GetSingleton();
                auto* location = player ? player->GetCurrentLocation() : nullptr;
                Add("cleared", location ? std::format("Cleared {}", SafeStr(location->GetFullName())) : "Cleared a location");
                return Result::kContinue;
            }

            Result ProcessEvent(const RE::DragonSoulsGained::Event*, RE::BSTEventSource<RE::DragonSoulsGained::Event>*) override
            {
                Add("dragon_soul", "Absorbed a dragon soul");
                return Result::kContinue;
            }

            Result ProcessEvent(const RE::LevelIncrease::Event* e, RE::BSTEventSource<RE::LevelIncrease::Event>*) override
            {
                if (e) {
                    Add("level_up", std::format("Reached level {}", e->newLevel));
                }
                return Result::kContinue;
            }
        };
    }

    void Install()
    {
        auto* sinks = Sinks::Get();
        if (auto* scripts = RE::ScriptEventSourceHolder::GetSingleton()) {
            scripts->AddEventSink<RE::TESActivateEvent>(sinks);
            scripts->AddEventSink<RE::TESContainerChangedEvent>(sinks);
        }
        RE::ActorKill::GetEventSource()->AddEventSink(sinks);
        RE::ItemsPickpocketed::GetEventSource()->AddEventSink(sinks);
        RE::LocationDiscovery::GetEventSource()->AddEventSink(sinks);
        RE::LocationCleared::GetEventSource()->AddEventSink(sinks);
        RE::DragonSoulsGained::GetEventSource()->AddEventSink(sinks);
        RE::LevelIncrease::GetEventSource()->AddEventSink(sinks);
        logger::info("event sinks installed");
    }

    void OnGameLoaded()
    {
        g_bountiesKnown = false;
    }

    void PollCrime(RE::TESDataHandler* data)
    {
        std::map<RE::FormID, std::int32_t> now;
        for (auto* faction : data->GetFormArray<RE::TESFaction>()) {
            if (faction && (faction->data.flags & RE::FACTION_DATA::kTrackCrime)) {
                now[faction->GetFormID()] = faction->GetCrimeGoldViolent() + faction->GetCrimeGoldNonViolent();
            }
        }
        if (g_bountiesKnown) {
            for (const auto& [id, gold] : now) {
                const auto before = g_bounties[id];
                if (gold > before) {
                    auto* faction = RE::TESForm::LookupByID<RE::TESFaction>(id);
                    Add("crime", std::format("Bounty +{} ({})", gold - before, faction ? SafeStr(faction->GetFullName()) : "?"));
                }
            }
        }
        g_bounties = std::move(now);
        g_bountiesKnown = true;
    }

    std::uint64_t Session() { return g_session; }

    json Recent()
    {
        std::scoped_lock lock(g_lock);
        json out = json::array();
        for (const auto& e : g_events) {
            out.push_back(e);
        }
        return out;
    }
}
