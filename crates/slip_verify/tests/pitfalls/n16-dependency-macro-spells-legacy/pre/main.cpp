#include "dep.h"

namespace legacy {
struct Mine {};
}


void user() { LEGACY_NAME(Mine) m; }
