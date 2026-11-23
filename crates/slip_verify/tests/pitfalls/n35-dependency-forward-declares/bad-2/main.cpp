#include "dep.h"

namespace acme {
struct Mine { int v; };
struct Other {};
}

void user() { acme::Mine m; m.v = 1; acme::Other o; }
