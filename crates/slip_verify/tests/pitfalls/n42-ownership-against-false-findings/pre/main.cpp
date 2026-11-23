#include "dep.h"

namespace legacy {
LEGACY_DECLARE(Mine)
}

int user() { return legacy::Mine().v; }
