// Reads game state through CommonLibSSE-NG. Field names and enum strings must
// match crates/protocol/src/lib.rs.

#include "snapshot.h"
#include "events.h"

#include <algorithm>
#include <chrono>
#include <cctype>
#include <cmath>
#include <format>
#include <numbers>
#include <optional>
#include <unordered_map>

#include <nlohmann/json.hpp>

using json = nlohmann::json;

namespace companion
{
    namespace
    {
        constexpr int kProtocolVersion = 1;
        constexpr RE::FormID kTamrielWorldspace = 0x0000003C;

        std::string SafeStr(const char* s) { return s ? s : ""; }

        json Vec3(const RE::NiPoint3& p) { return { { "x", p.x }, { "y", p.y }, { "z", p.z } }; }

        json Player(RE::PlayerCharacter* player)
        {
            json j;
            j["name"] = SafeStr(player->GetName());
            j["level"] = player->GetLevel();

            // PlayerSkills::Data holds the XP bar: xp towards the next level and the threshold.
            if (auto* skills = player->GetInfoRuntimeData().skills; skills && skills->data) {
                j["xp"] = skills->data->xp;
                j["xp_next"] = skills->data->levelThreshold;
            }

            j["gold"] = player->GetGoldAmount();
            if (auto* av = player->AsActorValueOwner()) {
                j["carry_weight"] = av->GetActorValue(RE::ActorValue::kInventoryWeight);
                j["max_carry_weight"] = av->GetActorValue(RE::ActorValue::kCarryWeight);
            }

            j["position"] = Vec3(player->GetPosition());
            j["heading"] = player->data.angle.z;
            // The camera node looks along its local +Y axis; project that onto the ground plane.
            if (auto* camera = RE::PlayerCamera::GetSingleton(); camera && camera->cameraRoot) {
                const auto& m = camera->cameraRoot->world.rotate;
                const float yaw = std::atan2(m.entry[0][1], m.entry[1][1]);
                j["camera_heading"] = yaw < 0.0f ? yaw + 2.0f * std::numbers::pi_v<float> : yaw;
            }

            auto* cell = player->GetParentCell();
            const bool interior = cell && cell->IsInteriorCell();
            j["is_interior"] = interior;
            j["cell"] = cell ? SafeStr(cell->GetFullName()) : "";

            std::string worldspace;
            if (auto* ws = player->GetWorldspace(); ws && !interior) {
                worldspace = ws->GetFormID() == kTamrielWorldspace ? "Tamriel" : SafeStr(ws->GetFormEditorID());
            }
            j["worldspace"] = worldspace;

            auto* location = player->GetCurrentLocation();
            j["location"] = location ? SafeStr(location->GetFullName()) : "";
            return j;
        }

        json Time()
        {
            json j;
            if (auto* cal = RE::Calendar::GetSingleton()) {
                j["hour"] = cal->GetHour();
                j["day"] = static_cast<std::uint32_t>(cal->GetDay());
                j["month"] = cal->GetMonth();
                j["year"] = cal->GetYear();
                j["day_of_week"] = cal->GetDayOfWeek();
                j["days_passed"] = cal->GetDaysPassed();
                j["timescale"] = cal->GetTimescale();
            }
            return j;
        }

        json Crime(RE::TESDataHandler* data)
        {
            json bounties = json::array();
            for (auto* faction : data->GetFormArray<RE::TESFaction>()) {
                if (!faction || !(faction->data.flags & RE::FACTION_DATA::kTrackCrime)) {
                    continue;
                }
                const auto violent = faction->GetCrimeGoldViolent();
                const auto nonViolent = faction->GetCrimeGoldNonViolent();
                if (violent + nonViolent <= 0) {
                    continue;
                }
                bounties.push_back({
                    { "faction", SafeStr(faction->GetFullName()) },
                    { "violent", violent },
                    { "non_violent", nonViolent },
                });
            }
            return { { "bounties", bounties } };
        }

        const char* QuestKind(RE::QUEST_DATA::Type type)
        {
            using T = RE::QUEST_DATA::Type;
            switch (type) {
            case T::kMainQuest:
                return "main";
            case T::kMagesGuild:
            case T::kThievesGuild:
            case T::kDarkBrotherhood:
            case T::kCompanionsQuest:
            case T::kCivilWar:
                return "faction";
            case T::kDaedric:
                return "daedric";
            case T::kSideQuest:
                return "side";
            case T::kDLC01_Vampire:
            case T::kDLC02_Dragonborn:
                return "dlc";
            default:
                return "misc";
            }
        }

        // Returns nullptr for objectives the journal doesn't show.
        const char* ObjectiveState(RE::QUEST_OBJECTIVE_STATE state)
        {
            using S = RE::QUEST_OBJECTIVE_STATE;
            switch (state) {
            case S::kDisplayed:
                return "active";
            case S::kCompletedDisplayed:
                return "completed";
            case S::kFailedDisplayed:
                return "failed";
            default:
                return nullptr;
            }
        }

        bool EqualsNoCase(std::string_view a, std::string_view b)
        {
            return std::ranges::equal(a, b, [](char x, char y) { return std::tolower(static_cast<unsigned char>(x)) == std::tolower(static_cast<unsigned char>(y)); });
        }

        std::optional<std::string> GlobalText(RE::TESDataHandler* data, RE::TESQuest* quest, std::string_view id)
        {
            auto format = [](float v) {
                return std::abs(v - std::round(v)) < 0.001f ? std::format("{}", static_cast<long long>(std::round(v))) : std::format("{:.1f}", v);
            };
            if (quest->textGlobals) {
                for (auto* g : *quest->textGlobals) {
                    if (g && EqualsNoCase(SafeStr(g->GetFormEditorID()), id)) {
                        return format(g->value);
                    }
                }
            }
            // Globals never change editor IDs, so index them once (lower-cased).
            static const auto byId = [data] {
                std::unordered_map<std::string, RE::TESGlobal*> map;
                for (auto* g : data->GetFormArray<RE::TESGlobal>()) {
                    if (g) {
                        std::string key = SafeStr(g->GetFormEditorID());
                        std::ranges::transform(key, key.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
                        map.emplace(std::move(key), g);
                    }
                }
                return map;
            }();
            std::string key(id);
            std::ranges::transform(key, key.begin(), [](unsigned char c) { return static_cast<char>(std::tolower(c)); });
            const auto it = byId.find(key);
            return it != byId.end() ? std::optional(format(it->second->value)) : std::nullopt;
        }

        std::optional<std::string> AliasText(RE::TESQuest* quest, std::string_view name)
        {
            for (auto* alias : quest->aliases) {
                if (!alias || !EqualsNoCase(SafeStr(alias->aliasName.c_str()), name) ||
                    alias->GetVMTypeID() != RE::BGSRefAlias::VMTYPEID) {
                    continue;
                }
                auto* ref = static_cast<RE::BGSRefAlias*>(alias)->GetReference();
                if (const char* n = ref ? ref->GetDisplayFullName() : nullptr; n && *n) {
                    return std::string(n);
                }
            }
            return std::nullopt;
        }

        // Replaces the journal's text tokens, e.g. "(<Global=FavorCount>/<Global=FavorTotal>)" or
        // "<Alias=Target>", with their current values. Unknown tokens are left for the app to tidy.
        std::string ResolveText(RE::TESDataHandler* data, RE::TESQuest* quest, std::string_view text)
        {
            std::string out;
            std::size_t pos = 0;
            while (pos < text.size()) {
                const auto open = text.find('<', pos);
                const auto close = open == std::string_view::npos ? open : text.find('>', open);
                if (close == std::string_view::npos) {
                    out.append(text.substr(pos));
                    break;
                }
                out.append(text.substr(pos, open - pos));
                const auto token = text.substr(open + 1, close - open - 1);
                const auto eq = token.find('=');
                std::optional<std::string> value;
                if (eq != std::string_view::npos) {
                    const auto key = token.substr(0, eq);
                    const auto arg = token.substr(eq + 1);
                    if (EqualsNoCase(key, "Global")) {
                        value = GlobalText(data, quest, arg);
                    } else if (EqualsNoCase(key.substr(0, 5), "Alias")) {
                        value = AliasText(quest, arg);
                    }
                }
                out.append(value ? *value : std::string(text.substr(open, close - open + 1)));
                pos = close + 1;
            }
            return out;
        }

        // Every quest the journal could list, with where it stands for the player:
        // "tracked"/"current" (running, shown in the journal), "completed" (finished or failed),
        // or "unknown" (a named quest the player hasn't been given yet).
        json Quests(RE::TESDataHandler* data)
        {
            json quests = json::array();
            for (auto* quest : data->GetFormArray<RE::TESQuest>()) {
                if (!quest) {
                    continue;
                }
                json objectives = json::array();
                bool anyShown = false;
                for (auto* obj : quest->objectives) {
                    if (!obj) {
                        continue;
                    }
                    const char* state = ObjectiveState(obj->state.get());
                    if (!state) {
                        continue;
                    }
                    anyShown = true;
                    objectives.push_back({
                        { "index", obj->index },
                        { "text", ResolveText(data, quest, SafeStr(obj->displayText.c_str())) },
                        { "state", state },
                    });
                }
                const std::string name = SafeStr(quest->GetFullName());
                const char* status = nullptr;
                if (quest->IsCompleted() || (anyShown && !quest->IsRunning())) {
                    status = anyShown || !name.empty() ? "completed" : nullptr;
                } else if (anyShown) {
                    status = quest->IsActive() ? "tracked" : "current";
                } else if (!name.empty() && !quest->objectives.empty() && quest->GetType() != RE::QUEST_DATA::Type::kNone) {
                    status = "unknown";
                }
                // Completed quests with no name and nothing shown are background quests.
                if (!status || (std::string_view(status) == "completed" && !anyShown)) {
                    continue;
                }
                quests.push_back({
                    { "form_id", quest->GetFormID() },
                    { "editor_id", SafeStr(quest->GetFormEditorID()) },
                    { "name", ResolveText(data, quest, name) },
                    { "kind", QuestKind(quest->GetType()) },
                    { "tracked", quest->IsActive() && anyShown },
                    { "status", status },
                    { "objectives", objectives },
                });
            }
            return quests;
        }

        // True when `ref` is in the cell or worldspace the player is in, so its
        // coordinates are directly comparable with the player's.
        //
        // Persistent references in unloaded exterior cells (quest targets, map markers, the doors
        // on a teleport path) often have no parent cell, but still report their worldspace.
        bool SameSpace(RE::PlayerCharacter* player, RE::TESObjectREFR* ref)
        {
            auto* playerCell = player->GetParentCell();
            auto* refCell = ref->GetParentCell();
            const bool playerInside = playerCell && playerCell->IsInteriorCell();
            const bool refInside = refCell && refCell->IsInteriorCell();
            if (playerInside || refInside) {
                return playerCell == refCell;
            }
            auto* worldspace = player->GetWorldspace();
            return worldspace && worldspace == ref->GetWorldspace();
        }

        // Where the compass should point for a target: the target itself when it is in
        // the player's space, otherwise the door on the game's path towards it that is in
        // the player's space (either side of each teleport door is tried).
        std::optional<RE::NiPoint3> CompassPosition(RE::PlayerCharacter* player, RE::TESQuestTarget* target,
            RE::TESObjectREFR* ref)
        {
            if (SameSpace(player, ref)) {
                return ref->GetPosition();
            }
            for (const auto& link : target->teleportPath.teleportRefs) {
                if (!link.ref) {
                    continue;
                }
                if (SameSpace(player, link.ref)) {
                    return link.ref->GetPosition();
                }
                if (auto other = link.ref->extraList.GetTeleportLinkedDoor().get(); other && SameSpace(player, other.get())) {
                    return other->GetPosition();
                }
            }
            return std::nullopt;
        }

        // For the log: "<formID> in <cell> (interior|worldspace)".
        std::string Describe(RE::TESObjectREFR* ref)
        {
            if (!ref) {
                return "none";
            }
            auto* cell = ref->GetParentCell();
            auto* ws = ref->GetWorldspace();
            return std::format("{:08X} in {} ({})", ref->GetFormID(),
                cell ? SafeStr(cell->GetFormEditorID()) : "?",
                cell && cell->IsInteriorCell() ? "interior" : (ws ? SafeStr(ws->GetFormEditorID()) : "no worldspace"));
        }

        // Logs targets that get no marker, once per change, to diagnose missing compass markers.
        void LogDropped(const std::vector<std::string>& dropped)
        {
            static std::vector<std::string> last;
            if (dropped == last) {
                return;
            }
            last = dropped;
            for (const auto& d : dropped) {
                logger::info("markers: {}", d);
            }
        }

        // Quest target markers from the game's own compass list (tracked quests only).
        json Markers(RE::TESDataHandler* data, RE::PlayerCharacter* player)
        {
            json markers = json::array();
            std::vector<std::string> dropped;
            std::string listed;
            {
                RE::BSSpinLockGuard guard(player->GetQuestTargetsLock());
                for (const auto& [quest, targets] : player->GetQuestTargets()) {
                    if (!quest || !targets) {
                        continue;
                    }
                    listed += std::format(" {}({})", SafeStr(quest->GetFormEditorID()), targets->size());
                    for (auto* target : *targets) {
                        if (!target) {
                            continue;
                        }
                        RE::ObjectRefHandle handle;
                        target->GetTargetRef(handle, true, quest);
                        auto ref = handle.get();
                        if (!ref) {
                            dropped.push_back(std::format("{} alias {}: no target ref", SafeStr(quest->GetFormEditorID()), target->alias));
                            continue;
                        }
                        auto pos = CompassPosition(player, target, ref.get());
                        if (!pos) {
                            std::string path;
                            for (const auto& link : target->teleportPath.teleportRefs) {
                                path += " -> door " + Describe(link.ref);
                            }
                            dropped.push_back(std::format("{} alias {}: target {}, path:{}",
                                SafeStr(quest->GetFormEditorID()), target->alias, Describe(ref.get()),
                                path.empty() ? " (empty)" : path));
                            continue;
                        }
                        // Name the marker after the objective that owns this target.
                        std::string name = ResolveText(data, quest, SafeStr(quest->GetFullName()));
                        for (auto* obj : quest->objectives) {
                            if (obj && std::find(obj->targets, obj->targets + obj->numTargets, target) !=
                                           obj->targets + obj->numTargets) {
                                name = ResolveText(data, quest, SafeStr(obj->displayText.c_str()));
                                break;
                            }
                        }
                        markers.push_back({
                            { "name", name },
                            { "kind", "quest_target" },
                            { "position", Vec3(*pos) },
                            { "quest", quest->GetFormID() },
                        });
                    }
                }
            }

            dropped.insert(dropped.begin(), "compass list:" + listed);
            LogDropped(dropped);

            // The custom marker the player can place on the world map.
            if (auto custom = player->GetInfoRuntimeData().playerMapMarker.get(); custom && SameSpace(player, custom.get())) {
                markers.push_back({
                    { "name", "Custom marker" },
                    { "kind", "custom" },
                    { "position", Vec3(custom->GetPosition()) },
                    { "quest", nullptr },
                });
            }
            return markers;
        }

        // Map marker type as the snake_case string the viewer picks icons by.
        const char* PlaceKind(RE::MARKER_TYPE type)
        {
            using M = RE::MARKER_TYPE;
            switch (type) {
            case M::kCity:
            case M::kRiftenCastle:
            case M::kRiftenCapitol:
            case M::kWindhelmCastle:
            case M::kWindhelmCapitol:
            case M::kWhiterunCastle:
            case M::kWhiterunCapitol:
            case M::kSolitudeCastle:
            case M::kSolitudeCapitol:
            case M::kMarkarthCastle:
            case M::kMarkarthCapitol:
            case M::kWinterholdCastle:
            case M::kWinterholdCapitol:
            case M::kMorthalCastle:
            case M::kMorthalCapitol:
            case M::kFalkreathCastle:
            case M::kFalkreathCapitol:
            case M::kDawnstarCastle:
            case M::kDawnstarCapitol:
            case M::kDLC02RavenRock:
                return "city";
            case M::kTown:
                return "town";
            case M::kSettlement:
                return "settlement";
            case M::kCave:
                return "cave";
            case M::kCamp:
                return "camp";
            case M::kFort:
            case M::kDLC02CastleKarstaag:
                return "fort";
            case M::kNordicRuin:
            case M::kDLC02MiraakTemple:
                return "nordic_ruin";
            case M::kDwemerRuin:
                return "dwemer_ruin";
            case M::kShipwreck:
                return "shipwreck";
            case M::kGrove:
                return "grove";
            case M::kDragonLair:
                return "dragon_lair";
            case M::kFarm:
                return "farm";
            case M::kWoodMill:
                return "wood_mill";
            case M::kMine:
                return "mine";
            case M::kImperialCamp:
                return "imperial_camp";
            case M::kStormcloakCamp:
                return "stormcloak_camp";
            case M::kDoomstone:
            case M::kDLC02StandingStone:
                return "standing_stone";
            case M::kWheatMill:
                return "wheat_mill";
            case M::kSmelter:
                return "smelter";
            case M::kStable:
                return "stable";
            case M::kImperialTower:
                return "imperial_tower";
            case M::kClearing:
                return "clearing";
            case M::kPass:
                return "pass";
            case M::kAltar:
                return "altar";
            case M::kRock:
                return "rock";
            case M::kLighthouse:
                return "lighthouse";
            case M::kOrcStronghold:
                return "orc_stronghold";
            case M::kGiantCamp:
                return "giant_camp";
            case M::kShack:
                return "shack";
            case M::kNordicTower:
            case M::kDLC02TelvanniTower:
                return "nordic_tower";
            case M::kNordicDwelling:
                return "nordic_dwelling";
            case M::kDocks:
            case M::kDLC02ToSkyrim:
            case M::kDLC02ToSolstheim:
                return "docks";
            case M::kShrine:
                return "shrine";
            default:
                return "landmark";
            }
        }

        // Found (visible) world map markers in Tamriel. Map markers are persistent references,
        // so they all live in the worldspace's persistent cell. Rebuilt every few seconds since
        // walking that cell each snapshot is wasteful and the list rarely changes.
        const json& Places()
        {
            static json cache = json::array();
            static std::chrono::steady_clock::time_point next{};
            const auto now = std::chrono::steady_clock::now();
            if (now < next) {
                return cache;
            }
            next = now + 5s;

            json places = json::array();
            auto* tamriel = RE::TESForm::LookupByID<RE::TESWorldSpace>(kTamrielWorldspace);
            if (tamriel && tamriel->persistentCell) {
                tamriel->persistentCell->ForEachReference([&](RE::TESObjectREFR* ref) {
                    auto* extra = ref ? ref->extraList.GetByType<RE::ExtraMapMarker>() : nullptr;
                    auto* data = extra ? extra->mapData : nullptr;
                    if (data && data->flags.any(RE::MapMarkerData::Flag::kVisible)) {
                        places.push_back({
                            { "name", SafeStr(data->locationName.GetFullName()) },
                            { "kind", PlaceKind(data->type.get()) },
                            { "position", Vec3(ref->GetPosition()) },
                            { "discovered", data->flags.any(RE::MapMarkerData::Flag::kCanTravelTo) },
                        });
                    }
                    return RE::BSContainer::ForEachResult::kContinue;
                });
            }
            cache = std::move(places);
            return cache;
        }

        bool InGame(RE::PlayerCharacter* player)
        {
            auto* ui = RE::UI::GetSingleton();
            if (!player || !player->Is3DLoaded() || !ui) {
                return false;
            }
            return !ui->IsMenuOpen(RE::MainMenu::MENU_NAME) && !ui->IsMenuOpen(RE::LoadingMenu::MENU_NAME);
        }
    }

    std::string BuildSnapshotJson()
    {
        json j;
        j["protocol"] = kProtocolVersion;
        j["game_version"] = REL::Module::get().version().string();

        auto* player = RE::PlayerCharacter::GetSingleton();
        auto* data = RE::TESDataHandler::GetSingleton();
        const bool inGame = InGame(player);
        j["in_game"] = inGame;
        j["session"] = events::Session();
        if (inGame && data) {
            j["player"] = Player(player);
            j["time"] = Time();
            j["crime"] = Crime(data);
            j["quests"] = Quests(data);
            j["markers"] = Markers(data, player);
            j["places"] = Places();
            events::PollCrime(data);
        }
        j["events"] = events::Recent();
        return j.dump(-1, ' ', false, json::error_handler_t::replace);
    }
}
