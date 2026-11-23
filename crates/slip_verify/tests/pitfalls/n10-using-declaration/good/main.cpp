#include "dep.h"

namespace acme {
struct Mine {};
}

using acme::Mine;

void user() { Mine m; }
