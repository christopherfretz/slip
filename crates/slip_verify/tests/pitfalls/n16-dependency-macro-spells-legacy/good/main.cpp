#include "dep.h"

namespace acme {
struct Mine {};
}

namespace legacy { using acme::Mine; }
void user() { LEGACY_NAME(Mine) m; }
