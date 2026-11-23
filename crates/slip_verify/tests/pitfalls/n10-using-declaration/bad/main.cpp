#include "dep.h"

namespace acme {
struct Mine {};
}

using legacy::Mine;

void user() { Mine m; }
