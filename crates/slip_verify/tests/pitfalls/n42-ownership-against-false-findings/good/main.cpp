#include "dep.h"

namespace acme {
LEGACY_DECLARE(Mine)
}

int user() { return acme::Mine().v; }
