#pragma once

#include <string>

namespace companion
{
    // Builds one JSON snapshot (see crates/protocol). Must run on the game thread.
    std::string BuildSnapshotJson();
}
