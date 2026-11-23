#include "dep.h"

namespace acme {
struct Mine {};
}


void user() { LEGACY_NAME(Mine) m; }
